"""Integration checks. Development only: Python 3 + Pillow."""
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
import zipfile
import zlib

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "scripts" / "pixel-review.exe"


class PixelReview(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.dir = Path(self.temp.name)
        self.source = self.dir / "像素 cat.png"
        self.rgba = Image.new("RGBA", (4, 3))
        self.rgba.putdata([(x*44, y*55, x+y, (x*83+y*31) % 256)
                           for y in range(3) for x in range(4)])
        self.rgba.save(self.source)

    def call(self, command, *options, code=0, source=None, scale=3):
        args = [EXE, command, "--source", source or self.source]
        if scale is not None and command not in ("matrix", "bounds","edit-matrix","copy"):
            args.extend(["--scale", str(scale)])
        args.extend(options)
        run = subprocess.run(args, capture_output=True)
        self.assertEqual(run.returncode, code, run.stdout + run.stderr)
        return json.loads(run.stdout)

    def test_auto_zoom_full_region_and_rounding(self):
        for target, scale in [(None,400),(9,2),(10,2),(11,3),(1,1)]:
            opts = [] if target is None else ["--target-long-edge", str(target)]
            out = self.call("zoom", *opts, scale=None)
            self.assertEqual(out["scale"], scale)
            self.assertEqual(out["target_long_edge"], target or 1600)
            with Image.open(out["preview"]["path"]) as actual:
                expected = self.rgba.resize((4*scale,3*scale), Image.Resampling.NEAREST)
                self.assertEqual(actual.size, expected.size)
                self.assertEqual(actual.tobytes(), expected.tobytes())
        opts = ["--region","2,0,1,3","--target-long-edge","11"]
        zoom = self.call("zoom", *opts, scale=None)
        self.assertEqual(zoom["scale"], 4)
        self.assertEqual(self.call("pick", *opts, "--pick-preview", "3,11",scale=None)["selection"]["source_xy"], [2,2])
        self.assertTrue(self.call("compare", *opts, scale=None)["comparison"]["match"])
        for bad in [["--target-long-edge","0"],["--scale","2","--target-long-edge","8"],["--region","0,0,0,0"]]:
            self.call("zoom", *bad, scale=None, code=2)

    def canvas(self, action, name, *flags, code=0):
        proc = subprocess.run([EXE, "canvas", action, "--canvas", name, *flags],
                              cwd=self.dir, capture_output=True, timeout=30)
        self.assertEqual(proc.returncode,code,proc.stdout+proc.stderr)
        return json.loads(proc.stdout)

    def open_canvas(self, name, source):
        result = self.canvas("open", name, "--source", source)
        self.addCleanup(subprocess.run, [EXE,"canvas","close","--canvas",name,"--discard"],
                        capture_output=True,timeout=10)
        return result

    def cli(self, *args, code=0):
        run = subprocess.run([EXE,*args],cwd=self.dir,capture_output=True,timeout=30)
        self.assertEqual(run.returncode,code,run.stdout+run.stderr)
        return json.loads(run.stdout)

    def test_review_sheet_columns_and_alpha(self):
        bg = Image.new("RGBA", (9,7), (21,71,113,124))
        bg.save(self.dir/"room.png")
        candidate = self.rgba.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        candidate.save(self.dir/"改.png")
        r = self.cli("review-sheet","--source","改.png","--before",self.source,
                     "--display-scale","2","--detail-scale","3","--region","1,1,2,2",
                     "--background","AF7755","--scene","room.png","--at","1,3","--out","board.png")
        self.assertEqual([c["role"] for c in r["columns"]],["before","candidate"])
        self.assertEqual([Path(c["path"]).name for c in r["columns"]],[self.source.name,"改.png"])
        with Image.open(r["preview"]["path"]) as im:
            for i,(column,src) in enumerate(zip(r["columns"],[self.rgba,candidate]),1):
                self.assertEqual(column["number"],i)
                flat=Image.new("RGBA",src.size,(175,119,85,255))
                flat.alpha_composite(src)
                scene=bg.copy()
                scene.alpha_composite(src,(1,3))
                for key,expected in [("display",flat.resize((8,6),Image.Resampling.NEAREST)),
                                     ("detail",flat.crop((1,1,3,3)).resize((6,6),Image.Resampling.NEAREST)),
                                     ("scene",scene)]:
                    p=column[key]
                    self.assertEqual((p["width"],p["height"]),expected.size)
                    self.assertEqual(im.crop((p["x"],p["y"],p["x"]+p["width"],p["y"]+p["height"])).tobytes(),
                                     expected.tobytes())
        opts=["review-sheet","--source",self.source]
        r=self.cli(*opts,"--source","改.png","--out","simple.png")
        self.assertEqual((r["display_scale"],r["detail_scale"],r["scene"]),(1,80,None))
        self.assertEqual([p["number"] for p in r["columns"]],[1,2])
        r=self.cli(*opts,"--scene","room.png","--out","center.png")
        self.assertEqual(r["at"],[2,2])
        original=(self.dir/"board.png").read_bytes()
        self.cli(*opts,"--out","board.png",code=2)
        self.assertEqual((self.dir/"board.png").read_bytes(),original)
        for flags in (["--at","0,0"],["--region","0,2,4,2"],["--source","room.png"],
                      ["--scene","room.png","--at","6,5"],["--display-scale","0"],["--detail-scale","0"]):
            self.cli(*opts,*flags,"--out","bad.png",code=2)
            self.assertFalse((self.dir/"bad.png").exists())

    def test_tile_repetition_and_wall_layout(self):
        crop = self.rgba.crop((1,0,3,2))
        opts = ["tile-preview","--source",self.source,"--region","1,0,2,2","--scale","2"]
        result = self.cli(*opts,"--out","repeat.png")
        self.assertEqual(result["grid"],[3,3])
        mosaic = Image.new("RGBA",(6,6))
        for r in range(3):
            for c in range(3): mosaic.paste(crop,(2*c,2*r))
        with Image.open(self.dir/"repeat.png") as actual:
            self.assertEqual(actual.tobytes(),mosaic.resize((12,12),Image.Resampling.NEAREST).tobytes())
        self.cli(*opts,"--out","repeat.png",code=2)
        self.cli(*opts,"--out","bad.png","--repeat","0,3",code=2)
        folder = self.dir/"walls"
        folder.mkdir()
        self.rgba.save(folder/"atlas.png")
        layout = {"tiles":{"角":{"source":"atlas.png","region":[0,0,2,2]},
                          "墙":{"source":"atlas.png","region":[2,0,2,2]}},"rows":["角墙","墙角"]}
        manifest = folder/"layout.json"
        manifest.write_text(json.dumps(layout),encoding="utf-8-sig")
        role = Image.new("RGBA",(1,2),(112,21,68,128))
        role.putpixel((0,1),(11,22,33,0))
        role.save(self.dir/"role.png")
        self.cli("tile-preview","--layout",manifest,"--scale","1","--overlay","role.png",
                 "--at","1,1","--out","room.png")
        expected = Image.new("RGBA",(4,4))
        for y,row in enumerate(layout["rows"]):
            for x,key in enumerate(row):
                sx = layout["tiles"][key]["region"][0]
                expected.paste(self.rgba.crop((sx,0,sx+2,2)),(2*x,2*y))
        expected.alpha_composite(role,(1,1))
        with Image.open(self.dir/"room.png") as actual:
            self.assertEqual(actual.tobytes(),expected.tobytes())
        for rows in ([],["角","角角"],["塔"]):
            manifest.write_text(json.dumps(dict(layout,rows=rows)))
            self.cli("tile-preview","--layout",manifest,"--out","bad.png",code=2)
        self.assertFalse((self.dir/"bad.png").exists())

    def test_animation_timing_alpha_and_footline(self):
        frames = [Image.new("RGBA",(4,3)) for _ in range(3)]
        frames[0].putpixel((0,2),(200,10,20,255))
        frames[1].putpixel((1,1),(5,60,90,128))
        args=[]
        for i,im in enumerate(frames):
            path=self.dir/f"{i}.png"
            im.save(path)
            args.extend(["--frame",path])
        report=self.cli("animation",*args,"--fps","5","--scale","2","--out","walk.apng")
        with Image.open(self.dir/"walk.apng") as actual:
            self.assertEqual((actual.n_frames,actual.size,actual.info["loop"]),(3,(8,6),0))
            for i,frame in enumerate(frames):
                actual.seek(i)
                self.assertEqual(actual.info["duration"],200)
                self.assertEqual(actual.convert("RGBA").tobytes(),frame.resize((8,6),Image.Resampling.NEAREST).tobytes())
        self.assertEqual([r["bottom_y"] for r in report["frames"]],[2,1,None])
        lined=self.cli("animation",*args,"--baseline","2","--scale","2","--out","guide.png")
        self.assertEqual([r["bottom_delta"] for r in lined["frames"]],[0,-1,None])
        with Image.open(self.dir/"guide.png") as im:
            im.seek(2)
            self.assertEqual(im.getpixel((0,5)),(255,200,50,255))
            self.assertEqual(im.getpixel((0,4)),(0,0,0,0))
            self.assertEqual(im.getpixel((4,5)),(0,0,0,0))
        Image.new("RGBA",(3,4)).save(self.dir/"wrong.png")
        for extra in (["--fps","0"],["--baseline","3"],["--frame","wrong.png"]):
            self.cli("animation",*args,*extra,"--out","bad.png",code=2)
            self.assertFalse((self.dir/"bad.png").exists())

    def test_animation_config_and_numbered_strip(self):
        alt = self.rgba.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        alt.save(self.dir/"next.png")
        config = self.dir/"configs"/"步态.json"
        frames = ["--frame", self.source, "--frame", "next.png", "--frame", self.source]
        self.cli("animation-config", *frames, "--fps", "5", "--baseline", "2", "--out", config)
        doc = json.loads(config.read_bytes())
        self.assertEqual([(config.parent/p).resolve() for p in doc["frames"]],
                         [self.source, self.dir/"next.png", self.source])
        self.assertTrue(all(not Path(p).is_absolute() for p in doc["frames"]))
        opts = ["--config", config, "--target-long-edge", "48"]
        r = self.cli("animation", *opts, "--no-baseline", "--out", "walk.apng")
        self.assertEqual((r["fps"],r["baseline"],r["scale"],r["strip"]["scale"]), (5,None,12,4))
        with Image.open(r["strip"]["path"]) as strip:
            labels = set()
            for i, src in enumerate([self.rgba,alt,self.rgba]):
                box = r["frames"][i]["strip_region"]
                x,y,w,h = (box[k] for k in ("x","y","width","height"))
                self.assertEqual(strip.crop((x,y,x+w,y+h)).tobytes(),
                                 src.resize((w,h),Image.Resampling.NEAREST).tobytes())
                label = strip.crop((x,0,x+w,y))
                self.assertGreater(len(label.getcolors()),1)
                labels.add(label.tobytes())
            self.assertEqual(len(labels),3)
        onion = self.cli("onion", *opts,"--index","2","--out","onion.png")
        self.assertEqual((onion["baseline"],onion["frames"][0]["index"]), (2,2))
        custom = self.cli("animation", *opts,"--fps","10","--baseline","1","--strip-out","custom.png","--out","fast.png")
        self.assertEqual((custom["fps"],custom["baseline"]), (10,1))
        with Image.open(self.dir/"fast.png") as im:
            self.assertEqual((im.n_frames, im.info["duration"]), (3,100))
        self.cli("animation",*opts,"--strip-out","custom.png","--out","occupied.png",code=2)
        self.assertFalse((self.dir/"occupied.png").exists())
        no_strip=self.cli("animation",*opts,"--no-strip","--out","single.apng")
        self.assertIsNone(no_strip["strip"])
        self.assertFalse((self.dir/"single.frames.png").exists())
        for extra in (["--frame",self.source],["--strip-out","bad.png"],["--no-strip","--strip-out","unused.png"]):
            self.cli("animation",*opts,*extra,"--out","bad.png",code=2)
        self.assertFalse((self.dir/"bad.png").exists())

    def test_resize_png_and_canvas(self):
        self.rgba.putpixel((2,1),(99,70,3,0))
        self.rgba.save(self.source)
        edges=["--left","2","--right","-1","--top","1","--bottom","2","--baseline","2"]
        r=self.cli("resize","--source",self.source,*edges,"--out","pad.png")["resize"]
        self.assertEqual((r["new_size"],r["origin_shift"],r["new_baseline"]),([5,6],[2,1],3))
        expected=Image.new("RGBA",(5,6))
        expected.paste(self.rgba,(2,1))
        with Image.open(self.dir/"pad.png") as im:
            self.assertEqual(im.tobytes(),expected.tobytes())
        r=self.cli("resize","--source","pad.png","--crop","2,1,3,1","--baseline","3","--out","cut.png")
        self.assertFalse(r["resize"]["baseline_visible"])
        with Image.open(self.dir/"cut.png") as im:
            self.assertEqual(im.tobytes(),self.rgba.crop((0,0,3,1)).tobytes())
        for invalid in (["--left","-4"],["--crop","4,0,2,1"],["--left","1","--crop","0,0,2,1"]):
            self.cli("resize","--source",self.source,*invalid,"--out","bad.png",code=2)
        name=self.dir.name
        self.open_canvas(name,self.source)
        op=lambda action,*args:self.canvas(action,name,*args)
        op("resize",*edges)
        patch=self.dir/"paint.json"
        patch.write_text(json.dumps({"ops":[{"points":[[0,5]],"color":"112233"}]}))
        op("edit","--patch",patch)
        op("undo")
        self.assertFalse(op("undo")["dirty"])
        op("export","--out","restored.png")
        with Image.open(self.dir/"restored.png") as im:
            self.assertEqual(im.tobytes(),self.rgba.tobytes())
        op("redo")
        op("redo")
        op("save","--out","finished.png")
        expected.putpixel((0,5),(17,34,51,255))
        with Image.open(self.dir/"finished.png") as im:
            self.assertEqual(im.tobytes(),expected.tobytes())

    def test_resize_layered_project(self):
        name=self.dir.name
        self.open_canvas(name,self.source)
        op=lambda action,*args:self.canvas(action,name,*args)
        op("layer-add","--layer","hidden","--source",self.source,"--at","-1,2")
        op("layer-set","--layer","hidden","--visible","false")
        op("save","--project","layers")
        before=op("status")["layers"]
        r=op("resize","--crop","1,1,2,1","--baseline","2")
        self.assertEqual([p["at"] for p in r["layers"]],[[-1,-1],[-2,1]])
        self.assertEqual(op("undo")["layers"],before)
        op("redo")
        op("save")
        op("close")
        self.open_canvas(name,self.dir/"layers")
        op("export","--out","small.png")
        with Image.open(self.dir/"small.png") as im:
            self.assertEqual(im.tobytes(),self.rgba.crop((1,1,3,2)).tobytes())
        r=op("resize","--left","1","--top","1","--right","1","--bottom","1")
        self.assertEqual(r["layers"],before)
        op("export","--out","large.png")
        with Image.open(self.dir/"large.png") as im:
            self.assertEqual(im.tobytes(),self.rgba.tobytes())
        self.canvas("resize",name,"--left","2","--pad-layer","hidden","--pad-layer","unknown",code=2)
        self.assertEqual(op("status")["layers"],before)
        widened=op("resize","--left","2","--pad-layer","hidden","--pad-layer","Base")["layers"]
        self.assertEqual([(l["at"],l["width"]) for l in widened],[([0,0],6),([-1,2],6)])
        op("export","--layer","hidden","--out","padded-layer.png")
        expected=Image.new("RGBA",(6,3))
        expected.paste(self.rgba,(2,0))
        with Image.open(self.dir/"padded-layer.png") as im:
            self.assertEqual(im.tobytes(),expected.tobytes())
        self.assertEqual(op("undo")["layers"],before)
        self.assertEqual(op("redo")["layers"],widened)

    def test_onion_temporal_direction_and_edges(self):
        frames=[Image.new("RGBA",(4,2)) for _ in range(3)]
        for i,im in enumerate(frames):
            im.putpixel((i,0),(110,70,8,255))
            im.putpixel((3,1),(11,19,30,255))
            im.save(self.dir/f"{i}.png")
        args=["--frame","0.png","--frame","1.png","--frame","2.png","--scale","1"]
        middle=self.cli("onion",*args,"--index","2","--out","middle.png")
        self.assertEqual(middle["frames"][0]["index"],2)
        with Image.open(self.dir/"middle.png") as im:
            self.assertEqual([im.getpixel((i,0)) for i in range(3)],
                             [(0,210,255,102),(110,70,8,255),(255,64,170,102)])
            self.assertEqual(im.getpixel((3,1)),(11,19,30,255))
        start=self.cli("onion",*args,"--index","1","--opacity","0","--out","first.png")
        self.assertIsNone(start["previous"])
        with Image.open(self.dir/"first.png") as im:
            self.assertEqual(im.tobytes(),frames[0].tobytes())
        for extra in (["--index","0"],["--index","4"],["--index","1","--opacity","101"]):
            self.cli("onion",*args,*extra,"--out","bad.png",code=2)

    def test_blank_palette_and_pixel_history(self):
        name=self.dir.name
        call=lambda action,*args,**kw: self.canvas(action,name,*args,**kw)
        self.cli("create","--size","0,2","--out","bad.png",code=2)
        self.cli("create","--size","1,2","--out","plain.png")
        self.cli("create","--size","2,2","--out","plain.png",code=2)
        opened=call("create","--size","20,40","--out","blank.png")
        self.addCleanup(subprocess.run,[EXE,"canvas","close","--canvas",name,"--discard"],capture_output=True)
        with Image.open(self.dir/"blank.png") as im:
            self.assertEqual((im.size,im.tobytes()),((20,40),bytes(20*40*4)))
        self.assertFalse(opened["dirty"])
        pal=self.dir/"pal.json"
        pal.write_text(json.dumps({"palette":{"H":"BBCDEE","E":"101020","_":"transparent"}}),encoding="utf-8-sig")
        patch=self.dir/"paint.json"
        def paint(x,y,s,code=0):
            patch.write_text(json.dumps({"ops":[{"at":[x,y],"rows":[s]}]}))
            return call("edit","--patch",patch,code=code)
        def exported(out):
            call("export","--out",out)
            with Image.open(self.dir/out) as im: return im.tobytes()
        call("palette-use","--palette",pal)
        paint(2,5,"HHH")
        hair=exported("hair.png")
        call("save","--project","scene")
        eye=paint(3,6,"EE")
        self.assertTrue(eye["dirty"])
        both=exported("both.png")
        paint(3,6,"H?",code=2)
        self.assertEqual(both,exported("after-failure.png"))
        self.assertFalse(call("undo")["dirty"])
        self.assertEqual(hair,exported("hair-again.png"))
        # A successful no-op preserves redo.
        self.assertEqual(paint(2,5,"HHH")["redo_steps"],1)
        call("redo")
        self.assertEqual(both,exported("both-again.png"))
        call("undo")
        self.assertEqual(paint(5,8,"H")["redo_steps"],0)
        call("redo",code=2)
        self.assertTrue(call("palette-check")["ok"])
        call("save")
        call("close")
        call("open","--source","scene")
        self.assertEqual(call("status")["undo_steps"],0)
        self.assertEqual(call("status")["palette"]["H"],"#BBCDEEFF")
        paint(0,0,"HE")
        self.assertTrue(call("palette-check")["ok"])

    def test_layer_and_paste_history(self):
        name=self.dir.name
        self.open_canvas(name,self.source)
        call=lambda act,*args,**kw:self.canvas(act,name,*args,**kw)
        call("copy","--point","1,1","--clip","c.json")
        call("layer-add","--layer","A","--size","3,1")
        call("paste","--clip","c.json","--at","0,0","--at","2,0")
        call("layer-set","--layer","A","--at","2,-1","--index","0","--visible","false")
        call("layer-select","--layer","Base")
        call("layer-remove","--layer","A")
        call("save","--project","layers")
        restored=call("undo")
        self.assertEqual(restored["layers"][0]["at"],[2,-1])
        self.assertEqual(call("undo")["active_layer"],"A")
        restored=call("undo")
        self.assertTrue(restored["layers"][1]["visible"])
        call("undo")
        call("export","--layer","A","--out","empty.png")
        with Image.open(self.dir/"empty.png") as im:self.assertEqual(im.tobytes(),bytes(12))
        self.assertEqual(len(call("undo")["layers"]),1)
        for _ in range(5): final=call("redo")
        self.assertFalse(final["dirty"])

    def test_cross_image_palette_audit_and_swap(self):
        red=(240,1,2,255)
        blue=(24,50,200,255)
        hidden=(9,8,7,0)
        tint=(240,1,2,127)
        original=[red,blue,tint,hidden]
        for name in ("front.png","back.png"):
            im=Image.new("RGBA",(4,1))
            im.putdata(original)
            im.save(self.dir/name)
        (self.dir/"palette.json").write_text('{"palette":{"R":"F00102","B":"1832C8"}}')
        opts=["--source","front.png","--source","back.png","--palette","palette.json"]
        check=self.cli("palette-check",*opts)
        self.assertFalse(check["ok"])
        for r in check["images"]:
            self.assertEqual(r["outside_pixels"],1)
            self.assertEqual(r["samples"],[{"rgba":"#F001027F","count":1,"first":[2,0]}])
        self.assertTrue(self.cli("palette-check",*opts,"--allow","F001027F")["ok"])
        args=["--map","@R=@B","--map","@B=@R","--out-dir","corrected"]
        self.cli("recolor",*opts,"--source","missing.png",*args,code=2)
        self.assertFalse((self.dir/"corrected").exists())
        results=self.cli("recolor",*opts,*args)["images"]
        for r in results:
            self.assertEqual(r["changed_pixels"],2)
            with Image.open(r["source"]) as src, Image.open(r["out"]) as dst:
                self.assertEqual(src.tobytes(),bytes(sum((list(p) for p in original),[])))
                self.assertEqual([dst.getpixel((i,0)) for i in range(4)],[blue,red,tint,hidden])

    def test_recolor_across_layers_is_one_undo(self):
        name = self.dir.name
        self.open_canvas(name,self.source)
        self.canvas("layer-add",name,"--layer","second","--source",self.source)
        opts = ["--map","2C000153=abcdef","--map","583705FF=abcdef"]
        result = self.canvas("recolor",name,*opts)
        self.assertEqual([r["changed_pixels"] for r in result["reports"]],[1,1])
        self.assertEqual(result["undo_steps"],2)
        self.canvas("undo",name)
        for layer in ("Base","second"):
            self.canvas("export",name,"--layer",layer,"--out",layer+".png")
            with Image.open(self.dir/(layer+".png")) as restored:
                self.assertEqual(restored.tobytes(),self.rgba.tobytes())
        self.canvas("redo",name)
        self.canvas("export",name,"--layer","second","--out","again.png")
        with Image.open(self.dir/"again.png") as changed:
            self.assertEqual(changed.getpixel((1,0)),(171,205,239,255))

    def test_resident_canvases_and_cross_process_clip(self):
        a,b = [self.dir.name + s for s in (" 甲"," 乙")]
        dst = self.dir / "map.png"
        base = Image.new("RGBA",(8,3),(100,110,120,255))
        base.save(dst)
        original = dst.read_bytes()
        first = self.open_canvas(a,self.source)
        second = self.open_canvas(b,dst)
        self.assertNotEqual(first["pid"],second["pid"])
        self.canvas("open",a,"--source",dst,code=2)
        sel = self.canvas("copy",a,"--rect","0,0,2,1","--point","2,1","--clip","piece.json")
        self.assertEqual(sel["selection"]["selected_pixels"],3)
        self.canvas("close",a)
        self.canvas("status",a,code=2)
        self.canvas("paste",b,"--clip","piece.json","--at","0,0","--at","7,0",code=2)
        self.assertFalse(self.canvas("status",b)["dirty"])
        self.canvas("paste",b,"--clip","piece.json","--at","0,0","--skip-transparent")
        result = self.canvas("paste",b,"--clip","piece.json","--at","3,1","--at","5,1")
        self.assertEqual(result["pid"],second["pid"])
        self.assertTrue(result["dirty"])
        expected = base.copy()
        for dx,dy in [(0,0),(3,1),(5,1)]:
            for x,y in [(0,0),(1,0),(2,1)]:
                if (dx,x,y) != (0,0,0):
                    expected.putpixel((dx+x,dy+y),self.rgba.getpixel((x,y)))
        self.canvas("close",b,code=2)
        preview = self.canvas("preview",b,"--region","0,0,4,2","--scale","3","--out","detail.png")
        with Image.open(preview["preview"]["path"]) as view:
            self.assertEqual(view.tobytes(),expected.crop((0,0,4,2)).resize((12,6),Image.Resampling.NEAREST).tobytes())
        self.assertEqual(dst.read_bytes(),original)
        self.canvas("copy",b,"--point","1,0","--clip","memory.json")
        self.assertEqual(json.loads((self.dir/"memory.json").read_bytes())[3][0][2][0],list(expected.getpixel((1,0))))
        self.assertFalse(self.canvas("save",b)["dirty"])
        self.canvas("close",b)
        with Image.open(dst) as actual:
            self.assertEqual(actual.tobytes(),expected.tobytes())

    def test_canvas_save_conflict_and_save_as(self):
        name = self.dir.name
        self.open_canvas(name,self.source)
        self.canvas("copy",name,"--point","1,0","--clip","p.json")
        self.canvas("paste",name,"--clip","p.json","--at","0,0")
        Image.new("RGBA",(4,3)).save(self.source)
        changed = self.source.read_bytes()
        self.canvas("save",name,code=2)
        self.canvas("save",name,"--out",self.source,code=2)
        self.assertTrue(self.canvas("status",name)["dirty"])
        saved = self.canvas("save",name,"--out","recovered.png")
        self.assertFalse(saved["dirty"])
        self.assertEqual(Path(saved["source"]["path"]),self.dir/"recovered.png")
        self.assertEqual(self.source.read_bytes(),changed)
        self.canvas("paste",name,"--clip","p.json","--at","3,2")
        self.canvas("close",name,"--discard")
        with Image.open(self.dir/"recovered.png") as actual:
            self.assertEqual(actual.getpixel((3,2)),self.rgba.getpixel((3,2)))

    def test_layered_project_roundtrip(self):
        a = self.dir.name
        self.open_canvas(a,self.source)
        sprite = Image.new("RGBA",(2,2),(255,11,25,128))
        sprite.putpixel((0,1),(12,13,14,0))
        sprite.save(self.dir/"sprite.png")
        self.canvas("layer-add",a,"--layer","人物","--source","sprite.png","--at","1,0")
        self.canvas("save",a,code=2)
        self.canvas("export",a,"--out","scene.png")
        self.canvas("save",a,"--project","村庄")
        root = self.dir/"村庄"
        old = json.loads((root/"project.json").read_bytes())
        self.assertEqual(old["active"],"人物")
        with Image.open(self.dir/"scene.png") as im:
            expected = self.rgba.copy()
            expected.alpha_composite(sprite,(1,0))
            self.assertEqual(im.tobytes(),expected.tobytes())
        # Move partly outside the canvas. Local export retains clipped pixels.
        self.canvas("layer-set",a,"--layer","人物","--at","-1,1")
        self.canvas("export",a,"--out","moved.png")
        self.canvas("export",a,"--layer","人物","--out","solo.png")
        with Image.open(self.dir/"solo.png") as im:
            self.assertEqual(im.tobytes(),sprite.tobytes())
        with Image.open(self.dir/"moved.png") as im:
            self.assertEqual(im.getpixel((1,0)),self.rgba.getpixel((1,0)))
            self.assertEqual(im.getpixel((0,1)),Image.alpha_composite(self.rgba.crop((0,1,1,2)),sprite.crop((1,0,2,1))).getpixel((0,0)))
        self.canvas("layer-set",a,"--layer","人物","--visible","false")
        self.canvas("save",a)
        self.canvas("close",a)
        self.open_canvas(a,root)
        state = self.canvas("status",a)
        self.assertEqual(state["layers"][1]["at"],[-1,1])
        self.assertFalse(state["layers"][1]["visible"])
        self.assertEqual(state["active_layer"],"人物")
        self.canvas("preview",a,"--region","1,0,2,2","--scale","3","--out","crop.png")
        with Image.open(self.dir/"crop.png") as im:
            self.assertEqual(im.tobytes(),self.rgba.crop((1,0,3,2)).resize((6,6),Image.Resampling.NEAREST).tobytes())
        before = json.loads((root/"project.json").read_bytes())
        self.assertEqual([i["file"] for i in old["layers"]],[i["file"] for i in before["layers"]])
        patch = self.dir/"patch.json"
        patch.write_text(json.dumps({"ops":[{"points":[[0,0]],"color":"transparent"},
                                            {"points":[[2,2]],"color":"abcdef"}]}))
        self.canvas("edit",a,"--patch",patch,code=2)
        self.assertFalse(self.canvas("status",a)["dirty"])
        self.canvas("export",a,"--layer","人物","--out","verify.png")
        with Image.open(self.dir/"verify.png") as im:
            self.assertEqual(im.tobytes(),sprite.tobytes())
        patch.write_text(json.dumps({"palette":{"_":"transparent"},"ops":[{"at":[0,0],"rows":["_."]}]}))
        self.canvas("edit",a,"--patch",patch)
        self.canvas("layer-set",a,"--layer","人物","--index","0","--visible","true")
        self.canvas("save",a)
        saved = json.loads((root/"project.json").read_bytes())
        self.assertEqual([l["name"] for l in saved["layers"]],["人物","Base"])
        self.assertEqual(saved["layers"][1]["file"],old["layers"][0]["file"])
        with Image.open(root/saved["layers"][0]["file"]) as im:
            self.assertEqual(im.getpixel((0,0)),(0,0,0,0))
            self.assertEqual(im.getpixel((1,0)),sprite.getpixel((1,0)))
        self.canvas("export",a,"--layer","Base","--out","base.png")
        with Image.open(self.dir/"base.png") as im:
            self.assertEqual(im.tobytes(),self.rgba.tobytes())

    def test_project_conflict_save_as_and_blank_layer(self):
        a,b = (self.dir.name+c for c in "ab")
        self.open_canvas(a,self.source)
        self.canvas("layer-add",a,"--layer","物品","--size","2,1")
        self.canvas("save",a,"--project","project")
        self.open_canvas(b,self.dir/"project")
        self.canvas("layer-set",a,"--layer","物品","--at","3,2")
        self.canvas("save",a)
        self.canvas("layer-remove",b,"--layer","Base")
        self.canvas("save",b,code=2)
        self.canvas("save",b,"--project","recovered")
        doc = json.loads((self.dir/"recovered/project.json").read_bytes())
        self.assertEqual([l["name"] for l in doc["layers"]],["物品"])
        with Image.open(self.dir/"recovered"/doc["layers"][0]["file"]) as im:
            self.assertEqual((im.size,im.tobytes()),((2,1),bytes(8)))

    def test_auto_zoom_large_canvas_and_change_crop(self):
        canvas = Image.new("RGBA",(3000,2000),(5,17,23,0))
        canvas.putpixel((2999,1999),(1,2,3,255))
        canvas.save(self.source)
        out = self.call("zoom", scale=None)
        self.assertEqual(out["scale"],1)
        with Image.open(out["preview"]["path"]) as actual:
            self.assertEqual(actual.size,canvas.size)
            self.assertEqual(actual.tobytes(),canvas.tobytes())
        out = self.call("edit","--set-source","0,0=abcdef","--target-long-edge","10",scale=None)
        crop = out["change_preview"]
        self.assertEqual((out["scale"],crop["scale"],crop["width"],crop["height"]),(1,3,9,9))
        self.assertTrue(self.call("compare","--region","0,0,3,3","--preview",crop["path"],
                                  source=out["source"]["path"],scale=crop["scale"])["comparison"]["match"])

    def test_sparse_clipboard_anchor_and_mirrors(self):
        # Top-left corner of the bounding box is a hole. One selected pixel
        # has alpha zero and a nonzero hidden RGB value.
        self.rgba.putpixel((1, 0), (11, 22, 33, 0))
        self.rgba.save(self.source)
        clip = self.dir / "选区.json"
        opts = ["--point", "1,0", "--point", "2,0", "--rect", "2,0,1,1",
                "--rect", "0,1,1,1", "--out", clip]
        result = self.call("copy", *opts)
        self.assertEqual(result["selection"], dict(source_anchor=[0,0], size=[3,2], selected_pixels=3))
        self.call("copy", *opts, code=2)
        background = Image.new("RGBA", (6, 5), (3, 9, 70, 221))
        canvas = self.dir / "目标.png"
        background.save(canvas)
        for fx, fy in [(False, False),(True, False),(False, True),(True, True)]:
            flags = (["--flip-x"] if fx else []) + (["--flip-y"] if fy else [])
            dest = self.dir / f"paste-{fx}-{fy}.png"
            out = self.call("edit","--paste",clip,"--at","2,3",*flags,"--edited-source",dest,source=canvas)
            expected = background.copy()
            for x, y in [(1,0), (2,0), (0,1)]:
                expected.putpixel((2 + (2-x if fx else x), 3 + (1-y if fy else y)), self.rgba.getpixel((x,y)))
            with Image.open(dest) as actual:
                self.assertEqual(actual.tobytes(), expected.tobytes())
            self.assertEqual(out["selection"]["target_anchor"], [2,3])
            self.assertEqual(out["change_preview"]["changed_pixels"], 3)
            self.assertTrue(self.call("compare", source=dest)["comparison"]["match"])

    def test_move_snapshot_overlap_and_bounds(self):
        opts = ["--rect","1,1,2,1","--point","3,1","--point","2,1"]
        for dx, dy in [(-1,0),(0,-1),(0,1),(0,0)]:
            dest = self.dir / f"shift-{dx}-{dy}.png"
            out = self.call("edit", *opts, "--move-by",f"{dx},{dy}","--edited-source",dest)
            expected = self.rgba.copy()
            for x in (1,2,3):
                expected.putpixel((x,1),(0,0,0,0))
            for x in (1,2,3):
                expected.putpixel((x+dx,1+dy), self.rgba.getpixel((x,1)))
            with Image.open(dest) as actual:
                self.assertEqual(actual.tobytes(),expected.tobytes())
            changed = sum(expected.getpixel((x,y)) != self.rgba.getpixel((x,y))
                          for y in range(expected.height) for x in range(expected.width))
            self.assertEqual(out["change_preview"]["changed_pixels"], changed)
        data = self.source.read_bytes()
        self.call("edit",*opts,"--move-by","1,0","--in-place",code=2)
        self.call("edit",*opts,"--move-by","0,1","--region","0,0,4,2","--in-place",code=2)
        self.call("edit","--move-by","0,1","--in-place",code=2)
        clip = self.dir / "one.json"
        self.call("copy","--point","1,1","--out",clip)
        self.call("edit","--paste",clip,"--at","4,1","--in-place",code=2)
        self.call("edit","--paste",clip,"--in-place",code=2)
        self.call("edit","--paste",clip,"--at","1,1","--set-source","0,0=AABBCC","--in-place",code=2)
        clip.write_text('["pixel-review.clip/1",[0,0],[1,1],[[1,0,[[0,0,0,0]]]]]')
        self.call("edit","--paste",clip,"--at","0,0","--in-place",code=2)
        self.assertEqual(self.source.read_bytes(),data)

    def test_actual_change_preview_for_full_grid(self):
        # Two nearby edits span the 256-pixel snapshot boundary in a 30-wide image.
        src = Image.new("RGBA", (30,20), (12,34,56,0))
        src.save(self.source)
        rows = ["A"*30 for _ in range(20)]
        rows[8] = "A"*15 + "BB" + "A"*13
        patch = self.dir / "eyes.json"
        patch.write_text(json.dumps({"palette":{"A":"0C223800","B":"ABBCCD00"},
            "ops":[{"at":[0,0],"rows":rows},
                   {"points":[[29,19]],"color":"FFFFFF"},
                   {"points":[[29,19]],"color":"0C223800"}]}))
        result = self.call("edit","--patch",patch)
        cp = result["change_preview"]
        self.assertEqual(cp["status"], "generated")
        self.assertEqual(cp["changed_pixels"], 2)
        self.assertEqual(cp["bounds"], dict(x=15,y=8,width=2,height=1))
        self.assertEqual(cp["region"], dict(x=13,y=6,width=6,height=5))
        self.assertEqual((cp["width"],cp["height"]), (18,15))
        self.assertEqual(result["patch"]["pixel_writes"],602)
        with Image.open(result["source"]["path"]) as edited, Image.open(cp["path"]) as crop:
            expected = edited.crop((13,6,19,11)).resize((18,15), Image.Resampling.NEAREST)
            self.assertEqual(expected.tobytes(),crop.tobytes())
        repeat = self.call("edit","--patch",patch,source=result["source"]["path"])
        self.assertEqual(repeat["change_preview"], {"status":"unchanged","changed_pixels":0})
        self.assertFalse(Path(repeat["preview"]["path"]).with_suffix(".changes.png").exists())

    def test_change_preview_modes_clipping_and_collision(self):
        for name,pad,region in [("zero",0,(0,0,1,1)),("edge",1,(0,0,2,2)),
                                ("max",4294967295,(0,0,4,3))]:
            result=self.call("edit","--region","0,0,1,1","--set-preview","0,0=AABBCC",
                             "--edited-source",self.dir/(name+".png"),"--out-name",name+"-full.png",
                             "--preview-changes","--padding",str(pad))
            cp=result["change_preview"]
            self.assertEqual(cp["region"], dict(zip(("x","y","width","height"),region)))
            self.assertEqual(Path(cp["path"]).name,name+"-full.changes.png")
            self.assertEqual(result["preview"]["width"],3)
        result = self.call("edit","--set-source","2,1=112233","--no-preview-changes")
        self.assertEqual(result["change_preview"], {"status":"disabled"})
        self.assertFalse(Path(result["preview"]["path"]).with_suffix(".changes.png").exists())
        before = self.source.read_bytes()
        self.call("edit","--in-place","--padding","-1","--set-source","1,0=FFFFFF",code=2)
        self.call("edit","--in-place","--preview-changes","--no-preview-changes","--set-source","1,0=FFFFFF",code=2)
        self.assertEqual(self.source.read_bytes(),before)
        colliding = self.dir / "sample.changes.png"
        self.call("edit","--edited-source",colliding,"--out-name","sample.png","--set-source","0,1=112233",code=2)
        self.assertFalse(colliding.exists())
        self.assertFalse((self.dir/"sample.png").exists())

    def test_batch_patch_and_matrix_roundtrip(self):
        patch = self.dir / "changes.json"
        patch.write_text(json.dumps({"palette":{"青":"#00AA99","_":"transparent"},
            "ops":[{"points":[[1,0],[2,0]],"color":"#FF0000"},
                   {"rects":[[1,0,1,2],[2,1,2,1]],"color":"33445566"},
                   {"at":[1,0],"rows":[".青","_." ]},
                   {"at":[2,1],"pixels":[[None,"#BEAD0100"]]}]}),encoding="utf-8-sig")
        before = self.source.read_bytes()
        original = self.dir/"matrix.json"
        self.call("matrix","--region","1,0,3,2","--out",original)
        edited_table = self.dir/"matrix2.json"
        self.call("edit-matrix","--patch",patch,"--out",edited_table,source=original)
        expected = self.rgba.copy()
        for p,rgba in [((1,0),(51,68,85,102)),((2,0),(0,170,153,255)),
                       ((1,1),(0,0,0,0)),((2,1),(51,68,85,102)),((3,1),(190,173,1,0))]:
            expected.putpixel(p,rgba)
        for i,batch in enumerate([patch,edited_table]):
            dest = self.dir/f"edited{i}.png"
            self.call("edit","--region","1,0,3,2","--patch",batch,"--edited-source",dest)
            with Image.open(dest) as im: self.assertEqual(im.tobytes(),expected.tobytes())
        self.assertEqual(before,self.source.read_bytes())
        self.call("edit-matrix","--patch",patch,"--out",edited_table,source=original,code=2)

    def test_batch_pick_preserves_both_coordinate_spaces(self):
        region = ("--region", "1,1,3,2")
        preview = Path(self.call("zoom", *region)["preview"]["path"])
        with Image.open(preview) as image:
            changed = image.copy()
        changed.putpixel((8,5), (17,34,51,0))
        changed.save(preview)
        options = [("--pick-preview","8,5"),("--pick-source","2,2"),
                   ("--pick-source","1,1"),("--pick-preview","8,5")]
        flags = [arg for pair in options for arg in pair]
        selections = self.call("pick", *region, *flags)["selections"]
        ordered = [options[i] for i in (1,2,0,3)]
        expected = [self.call("pick", *region, *pair)["selection"] for pair in ordered]
        self.assertEqual(selections, expected)
        self.assertEqual(selections[-1]["preview_rgba"], "#11223300")
        self.assertEqual(selections[-1]["source_xy"], [3,2])
        self.call("pick", *region, *flags, "--pick-source", "0,0", code=2)

    def test_batch_reads_and_replay(self):
        dest=self.dir/"batch.json"
        flags=["--point","3,1","--point","0,0","--region","2,2,2,1","--region","0,0,2,2"]
        obj=self.call("matrix",*flags)
        self.assertEqual([r["region"]["x"] for r in obj["regions"]],[3,0,2,0])
        for result in obj["regions"]:
            r=result["region"]
            self.assertEqual(result,self.call("matrix","--region",f"{r['x']},{r['y']},{r['width']},{r['height']}"))
        self.call("matrix",*flags,"--out",dest)
        converted = self.call("edit","--patch",dest)
        with Image.open(converted["source"]["path"]) as im:
            self.assertEqual(im.tobytes(),self.rgba.tobytes())

    def test_invalid_batch_is_atomic(self):
        patch=self.dir/"p.json"
        preview=Path(self.call("zoom")["preview"]["path"])
        snapshot=(self.source.read_bytes(),preview.read_bytes())
        for bad in [{"at":[0,0],"rows":[".",".."]},{"at":[0,0],"rows":["?"]},
                    {"rects":[[1,1,9,9]],"color":"000000"},{"points":[[-1,0]],"color":"transparent"},
                    {"at":[3,2],"pixels":[[None,None]]},{"at":[0,0],"pixels":[["not a color"]]}]:
            patch.write_text(json.dumps({"ops":[{"points":[[0,0]],"color":"ffffff"},bad]}))
            self.call("edit","--in-place","--patch",patch,code=2)
            self.assertEqual(snapshot,(self.source.read_bytes(),preview.read_bytes()))

    def test_matrix_coordinates_and_export(self):
        for x, y, w, h in [(0,0,4,3),(1,1,2,2),(0,2,4,1),(3,0,1,3)]:
            args = [] if (w,h)==(4,3) else ["--region",f"{x},{y},{w},{h}"]
            result = self.call("matrix", *args)
            self.assertEqual(result["region"], dict(x=x,y=y,width=w,height=h))
            expected = [["#" + bytes(self.rgba.getpixel((x+i,y+j))).hex().upper()
                         for i in range(w)] for j in range(h)]
            self.assertEqual(result["pixels"], expected)
        dest = self.dir / "像素.json"
        receipt = self.call("matrix","--out",dest)
        self.assertEqual(Path(receipt["out"]), dest)
        self.assertEqual(json.loads(dest.read_bytes()), self.call("matrix"))
        self.call("matrix","--out",dest,code=2)
        self.call("matrix","--region","4,0,1,1",code=2)

    def test_alpha_bounds(self):
        image = Image.new("RGBA",(6,5),(17,22,90,0))
        image.save(self.source)
        self.assertIsNone(self.call("bounds")["bounds"])
        self.assertEqual(self.call("matrix")["pixels"][0][0], "#11165A00")
        image.putpixel((4,3),(0,0,0,1))
        image.save(self.source)
        b = self.call("bounds")
        self.assertEqual(b["bounds"],dict(left=4,right=4,top=3,bottom=3,width=1,height=1))
        self.assertEqual(b["margins"],dict(left=4,right=1,top=3,bottom=1))
        image.putpixel((1,3),(0,0,0,255))
        image.putpixel((3,1),(0,0,0,127))
        image.save(self.source)
        self.assertEqual(self.call("bounds")["bounds"],dict(left=1,right=4,top=1,bottom=3,width=4,height=3))

    def test_mapping_edit_and_corruption(self):
        area = ("--region", "1,1,3,2")
        output = self.call("zoom", *area, "--out-dir", self.dir / "output", "--out-name", "big.png")
        preview = Path(output["preview"]["path"])
        reference = self.rgba.crop((1, 1, 4, 3)).resize((9, 6), Image.Resampling.NEAREST)
        with Image.open(preview) as actual:
            self.assertEqual(actual.tobytes(), reference.tobytes())
        reference.putpixel((8, 5), (255, 0, 255, 0))
        reference.save(preview)
        pv = ("--preview", preview)
        diff = self.call("compare", *area, *pv, code=1)["comparison"]
        self.assertEqual(diff["mismatch_pixels"], 1)
        self.assertEqual(diff["samples"][0]["source_xy"], [3, 2])
        sample = self.call("pick", *area, *pv, "--pick-preview", "8,5")["selection"]
        self.assertEqual(sample["preview_rgba"], "#FF00FF00")
        self.assertEqual(sample["preview_block_inclusive"], {"x": [6, 8], "y": [3, 5]})
        self.assertNotEqual(sample["source_rgba"], sample["preview_rgba"])
        self.assertEqual(self.call("pick", *area, *pv, "--pick-source", "3,2")["selection"]["preview_xy"], [6, 3])
        before = self.source.read_bytes()
        edited = self.call("edit", *area, "--set-preview", "8,5=#AABBCC00",
                           "--set-source", "1,1=000000")
        self.assertEqual(self.source.read_bytes(), before)
        new = Path(edited["source"]["path"])
        self.rgba.putpixel((3, 2), (170, 187, 204, 0))
        self.rgba.putpixel((1, 1), (0, 0, 0, 255))
        with Image.open(new) as actual:
            self.assertEqual(actual.tobytes(), self.rgba.tobytes())
        self.assertTrue(self.call("compare", *area, source=new)["comparison"]["match"])
        self.call("edit", "--in-place", "--set-source", "0,0=01020304", source=new)
        self.assertTrue(self.call("compare", source=new)["comparison"]["match"])
        with Image.open(new) as actual:
            self.assertEqual(actual.getpixel((0, 0)), (1, 2, 3, 4))

    def test_failures(self):
        self.call("zoom", "--out-name", self.source.name, code=2)
        self.call("zoom", scale=0, code=2)
        self.call("zoom", "--region", "3,0,2,1", code=2)
        self.call("zoom", "--out-name", "../escape.png", code=2)
        self.call("edit", "--set-source", "0,0=FFF", code=2)
        self.call("edit", "--set-source", "1,1=000000", "--set-preview", "3,3=ffffff", code=2)
        self.call("pick", "--pick-preview", "12,9", code=2)
        self.call("zoom")
        self.call("pick", "--pick-preview", "12,9", code=2)
        wrong = self.call("compare", "--preview", self.source, code=1)
        self.assertEqual(wrong["comparison"]["reason"], "dimensions")
        self.assertEqual(wrong["preview"]["width"], 4)
        self.assertFalse(list(self.dir.glob("*.lock")))

    def test_png_modes(self):
        for mode in ("1", "L", "LA", "P", "RGB", "RGBA"):
            image = self.rgba.convert(mode)
            if mode == "P":
                image.info["transparency"] = bytes([0, 100, 255])
            image.save(self.source)
            result = self.call("zoom", scale=1)
            with Image.open(self.source) as src, Image.open(result["preview"]["path"]) as pv:
                self.assertEqual(src.convert("RGBA").tobytes(), pv.tobytes(), mode)
                rgba = src.convert("RGBA")
                colors = self.call("matrix")["pixels"]
                self.assertEqual(bytes.fromhex("".join(c[1:] for row in colors for c in row)), rgba.tobytes())
                l,t,r,b = rgba.getchannel("A").getbbox()
                self.assertEqual(self.call("bounds")["bounds"],dict(left=l,top=t,right=r-1,bottom=b-1,width=r-l,height=b-t))
        Image.new("I;16", (2, 2), 1025).save(self.source)
        self.call("zoom", code=2)
        self.rgba.save(self.source, save_all=True, append_images=[Image.new("RGBA",(4,3),"red")])
        self.call("zoom", code=2)

    def test_interlaced(self):
        # Encode the seven Adam7 passes separately, each using filter None.
        passes = [(0,0,8,8),(4,0,8,8),(0,4,4,8),(2,0,4,4),
                  (0,2,2,4),(1,0,2,2),(0,1,1,2)]
        rows = bytearray()
        for x,y,dx,dy in passes:
            if x < 4:
                for j in range(y,3,dy):
                    rows.append(0)
                    for i in range(x,4,dx):
                        rows.extend(self.rgba.getpixel((i,j)))
        def chunk(t,d):
            return struct.pack(">I",len(d))+t+d+struct.pack(">I",zlib.crc32(t+d))
        self.source.write_bytes(b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",4,3,8,6,0,0,1))
                                +chunk(b"IDAT",zlib.compress(rows))+chunk(b"IEND",b""))
        result=self.call("zoom",scale=1)
        with Image.open(result["preview"]["path"]) as out:
            self.assertEqual(out.tobytes(),self.rgba.tobytes())
        self.assertTrue(self.call("compare","--preview",self.source,scale=1)["comparison"]["match"])
        self.assertEqual(self.call("matrix","--region","2,2,1,1")["pixels"],
                         [["#" + bytes(self.rgba.getpixel((2,2))).hex().upper()]])

    def test_embedded_licenses(self):
        dest = self.dir / "licenses.zip"
        subprocess.run([EXE,"licenses","--out",dest], check=True, capture_output=True)
        expected = ROOT / "references" / "third-party.zip"
        self.assertEqual(hashlib.sha256(dest.read_bytes()).digest(), hashlib.sha256(expected.read_bytes()).digest())
        with zipfile.ZipFile(dest) as archive:
            self.assertIn("png 0.18.1",archive.read("INDEX.txt").decode())
            self.assertIsNone(archive.testzip())


if __name__ == "__main__":
    unittest.main()

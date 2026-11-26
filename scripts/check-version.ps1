# Check the installed package against version.json on GitHub main.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$repository = 'https://github.com/mtshang/pixel-review'
$url = 'https://api.github.com/repos/mtshang/pixel-review/contents/version.json?ref=main'

function Read-Version([string]$Json) {
    $manifest = ConvertFrom-Json -InputObject $Json
    if (-not $Json.TrimStart().StartsWith('{') -or
        $manifest.version -isnot [string] -or
        $manifest.version -notmatch '\A(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\z') {
        throw 'Expected a version string in major.minor.patch format.'
    }
    return [version]$manifest.version
}

$local = $null
$cloud = $null
$localError = $null
$cloudError = $null
try {
    $file = Join-Path (Split-Path -Parent $PSScriptRoot) 'version.json'
    $local = Read-Version ([IO.File]::ReadAllText($file))
} catch {
    $localError = 'Local version.json is missing, unreadable, or invalid.'
}

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $headers = @{ Accept = 'application/vnd.github+json'; 'Cache-Control' = 'no-cache' }
    $file = Invoke-RestMethod -Uri $url -Headers $headers -UserAgent 'pixel-review-version-check' -TimeoutSec 15
    if ($file.encoding -ne 'base64') { throw 'Unexpected GitHub file encoding.' }
    $bytes = [Convert]::FromBase64String($file.content)
    $cloud = Read-Version ([Text.Encoding]::UTF8.GetString($bytes).TrimStart([char]0xFEFF))
} catch {
    $cloudError = 'Could not retrieve a valid version.json from GitHub.'
    if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
        $cloudError += ' HTTP status: ' + [int]$_.Exception.Response.StatusCode + '.'
    }
}

$update = 'Unknown'
$code = 2
if ($null -ne $local -and $null -ne $cloud) {
    if ($cloud -gt $local) {
        $update = 'Yes'
        $code = 1
    } else {
        $update = 'No'
        $code = 0
    }
}

if ($null -eq $local) { $local = 'Unknown' }
if ($null -eq $cloud) { $cloud = 'Unknown' }
"Local version: $local"
"Cloud version: $cloud"
"GitHub link: $repository"
"Update needed: $update"
if ($localError) { "Error: $localError" }
if ($cloudError) { "Error: $cloudError" }
exit $code

<#
Fetch a pinned libmpv (zhongfly mpv-winbuild "mpv-dev" package) for the Windows
build + generate an MSVC import library from its DLL, so the shell can link libmpv
(the `libmpv` feature) and ship a self-contained mpv-2 next to the .exe.

The mpv-dev archive carries `libmpv-2.dll` + headers + a *MinGW* import lib
(`libmpv.dll.a`) - but NOT an MSVC `.lib`, and the project targets x86_64-pc-windows-
msvc. libmpv2-sys just emits `-lmpv` with no search path, so build.rs adds
KROMA_MPV_LIB_DIR to the link search (see build.rs). We therefore synthesise
`mpv.lib` from the DLL's export table using LLVM (pre-installed on windows runners) -
the exact flow validated on macOS: llvm-readobj -> .def -> llvm-dlltool.

Outputs (for the caller / CI):
  - <outdir>\mpv.lib          the MSVC import lib (named `mpv.lib` for -lmpv)
  - <outdir>\libmpv-2.dll     the runtime DLL to ship beside the .exe
  - sets KROMA_MPV_LIB_DIR (GITHUB_ENV) = <outdir>, consumed by build.rs

Idempotent: skips the download when the archive is already present with the right sha.
Bump VERSION_TAG + ASSET + SHA256 together to upgrade.
#>
$ErrorActionPreference = 'Stop'

# --- pin (bump the three together) --------------------------------------------
$VersionTag = '2026-07-21-94335ab87a'
$Asset      = 'mpv-dev-x86_64-20260721-git-94335ab87a.7z'
$Sha256     = '0377122DB231BF2AB1B708524C15F8D3FAF2E4FA8199318B33A921AEF7BBA83A'
# ------------------------------------------------------------------------------

$here   = Split-Path -Parent $MyInvocation.MyCommand.Path
$root   = Split-Path -Parent $here                    # clients/desktop
$outdir = Join-Path $root 'src-tauri\.libmpv-win'     # gitignored scratch
$archive = Join-Path $outdir $Asset
New-Item -ItemType Directory -Force -Path $outdir | Out-Null

function Get-Sha256($path) {
  (Get-FileHash -Algorithm SHA256 -Path $path).Hash.ToUpper()
}

if (-not (Test-Path $archive) -or (Get-Sha256 $archive) -ne $Sha256) {
  $url = "https://github.com/zhongfly/mpv-winbuild/releases/download/$VersionTag/$Asset"
  Write-Host "fetch-libmpv-windows: downloading $Asset"
  Invoke-WebRequest -Uri $url -OutFile $archive
  $got = Get-Sha256 $archive
  if ($got -ne $Sha256) {
    throw "fetch-libmpv-windows: sha256 MISMATCH for $Asset (got $got, want $Sha256)"
  }
} else {
  Write-Host "fetch-libmpv-windows: $Asset already present, sha256 OK"
}

# --- extract (7-Zip is pre-installed on windows runners) ----------------------
$sevenZip = @(
  'C:\Program Files\7-Zip\7z.exe',
  'C:\Program Files (x86)\7-Zip\7z.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $sevenZip) { $sevenZip = '7z' }  # else assume on PATH
& $sevenZip x "-o$outdir" -y $archive | Out-Null

$dll = Get-ChildItem -Path $outdir -Recurse -Filter 'libmpv-2.dll' | Select-Object -First 1
if (-not $dll) { throw "fetch-libmpv-windows: libmpv-2.dll not found in the archive" }
# Flatten: keep the DLL at the outdir root next to the import lib we build.
if ($dll.DirectoryName -ne $outdir) { Copy-Item $dll.FullName (Join-Path $outdir 'libmpv-2.dll') -Force }
$dllPath = Join-Path $outdir 'libmpv-2.dll'

# --- synthesise the MSVC import lib from the DLL export table -----------------
# LLVM ships on windows runners; find its bin dir if the tools aren't on PATH.
function Resolve-Tool($name) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  foreach ($p in @("$env:ProgramFiles\LLVM\bin\$name.exe", "$env:LLVM_PATH\bin\$name.exe")) {
    if ($p -and (Test-Path $p)) { return $p }
  }
  throw "fetch-libmpv-windows: $name not found (LLVM required)"
}
$readobj  = Resolve-Tool 'llvm-readobj'
$dlltool  = Resolve-Tool 'llvm-dlltool'

$def = Join-Path $outdir 'mpv.def'
$lib = Join-Path $outdir 'mpv.lib'
$exports = & $readobj '--coff-exports' $dllPath |
  Select-String -Pattern 'Name:\s*(\S+)' |
  ForEach-Object { $_.Matches[0].Groups[1].Value }
if (-not $exports -or $exports.Count -lt 10) {
  throw "fetch-libmpv-windows: too few exports parsed from libmpv-2.dll"
}
Set-Content -Path $def -Value (@('EXPORTS') + $exports) -Encoding ascii
& $dlltool '-m' 'i386:x86-64' '--input-def' $def '--dllname' 'libmpv-2.dll' '--output-lib' $lib
if (-not (Test-Path $lib)) { throw "fetch-libmpv-windows: failed to build mpv.lib" }

Write-Host "fetch-libmpv-windows: mpv.lib + libmpv-2.dll ready in $outdir"
# Hand the link-search dir to build.rs, and the DLL path to the bundling step.
if ($env:GITHUB_ENV) {
  Add-Content -Path $env:GITHUB_ENV -Value "KROMA_MPV_LIB_DIR=$outdir"
  Add-Content -Path $env:GITHUB_ENV -Value "KROMA_MPV_DLL=$dllPath"
}

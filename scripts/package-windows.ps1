param(
    [string]$BinaryPath = "",
    [string]$OutputDirectory = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$encoreRoot = Split-Path -Parent $PSScriptRoot
if (-not $BinaryPath) { $BinaryPath = Join-Path $encoreRoot "target/release/encore.exe" }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $encoreRoot "dist" }
if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    throw "Build first with cargo build --release --locked, or pass -BinaryPath."
}
$versionLine = Select-String -Path (Join-Path $encoreRoot "Cargo.toml") -Pattern '^version = "([^"]+)"$' | Select-Object -First 1
if (-not $versionLine) { throw "Could not read the package version from Cargo.toml." }
$encoreVersion = $versionLine.Matches[0].Groups[1].Value
$encoreArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
$encoreName = "Encore-$encoreVersion-windows-$encoreArchitecture"
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$encoreOutput = (Resolve-Path -LiteralPath $OutputDirectory).Path
$encoreStage = Join-Path $encoreOutput (".encore-package-" + [Guid]::NewGuid().ToString("N"))
$encorePackage = Join-Path $encoreStage $encoreName
New-Item -ItemType Directory -Path $encorePackage -Force | Out-Null
try {
    Copy-Item -LiteralPath $BinaryPath -Destination (Join-Path $encorePackage "Encore.exe")
    foreach ($encoreItem in @("assets", "src", "scripts", "Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "ARCHITECTURE.md", "BUILDING.md", "COMPATIBILITY.md", "FINALE_PARITY.md", "SHORTCUTS.md", "LINKED_PARTS.md", "PERCUSSION.md", "STAFF_GROUPS.md", "TREMOLOS.md", ".gitlab-ci.yml")) {
        Copy-Item -LiteralPath (Join-Path $encoreRoot $encoreItem) -Destination $encorePackage -Recurse
    }
    if (Test-Path -LiteralPath (Join-Path $encoreRoot "tests") -PathType Container) {
        Copy-Item -LiteralPath (Join-Path $encoreRoot "tests") -Destination $encorePackage -Recurse
    }
    $encoreExamples = Join-Path $encorePackage "examples"
    New-Item -ItemType Directory -Path $encoreExamples -Force | Out-Null
    foreach ($encorePattern in @("*.encore", "*.pdf", "*.png")) {
        Get-ChildItem -LiteralPath (Join-Path $encoreRoot "examples") -Filter $encorePattern -File | ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination $encoreExamples
        }
    }
    $encoreArchive = Join-Path $encoreOutput "$encoreName.zip"
    if (Test-Path -LiteralPath (Join-Path $encoreRoot "examples/Cantabile Parts") -PathType Container) {
        Copy-Item -LiteralPath (Join-Path $encoreRoot "examples/Cantabile Parts") -Destination $encoreExamples -Recurse
    }
    Compress-Archive -LiteralPath $encorePackage -DestinationPath $encoreArchive -Force
    Write-Output $encoreArchive
}
finally {
    Remove-Item -LiteralPath $encoreStage -Recurse -Force
}

[CmdletBinding()]
param(
    [string]$Runner = $(if ($env:OS -eq "Windows_NT") { ".\\target\\debug\\gaanim.exe" } else { "./target/debug/gaanim" })
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Runner -PathType Leaf)) {
    throw "Gaanim runner was not found: $Runner"
}

# These scripts only construct a scene and call render().  In --diff mode the
# embedded host accepts render() but then reports that no snapshots were
# requested. That expected diagnostic proves the public Python API executed.
# Export examples are covered separately because they intentionally invoke an
# encoder rather than the interactive host, and examples that capture
# snapshots are compared against their baselines instead.
$examples = @(
    "03_anchors.py",
    "advanced_animations_demo.py",
    "group_demo.py",
    "layout_verification.py",
    "math_animation.py",
    "number_plane_tangent.py",
    "reactive_features_demo.py",
    "scenes.py",
    "sine_curve.py",
    "sprint1_demo.py",
    "test_slides.py",
    "write_smoke.py"
)

foreach ($example in $examples) {
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = & $Runner --diff --example "examples/$example" --no-gui 2>&1 | Out-String
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousErrorAction
    if ($exitCode -ne 2 -or $output -notmatch "did not call scene\.snapshots") {
        throw "Example failed: $example`n$output"
    }
    Write-Host "validated $example"
}

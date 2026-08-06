[CmdletBinding()]
param(
    [string]$Runner = ".\\target\\debug\\gaanim.exe"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Runner -PathType Leaf)) {
    throw "Gaanim runner was not found: $Runner"
}

# These scripts only construct a scene and call render().  In --diff mode the
# embedded host accepts render() but then reports that no snapshots were
# requested. That expected diagnostic proves the public Python API executed.
# Export examples are covered separately because they intentionally invoke an
# encoder rather than the interactive host.
$examples = @(
    "03_anchors.py",
    "advanced_animations_demo.py",
    "boolean_ops.py",
    "group_demo.py",
    "layout_verification.py",
    "math_animation.py",
    "move_along_path.py",
    "number_plane_tangent.py",
    "reactive_features_demo.py",
    "scenes.py",
    "sine_curve.py",
    "sprint1_demo.py",
    "test_slides.py",
    "theme_demo.py",
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

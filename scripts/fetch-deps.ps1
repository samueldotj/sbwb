# Fetches build inputs that are not committed: the PDFium binary and the
# Tesseract English models. Verifies checksums recorded in this script.
# Usage: ./scripts/fetch-deps.ps1   (from the repository root)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

$pdfiumTag = "chromium/8044"
$pdfiumSha = "04100c03e41cac1f979e36e5e26fb860bcb5a7461f53830d3c098716624a27a9"
$models = @(
  @{ dir = "best"; url = "https://github.com/tesseract-ocr/tessdata_best/raw/main/eng.traineddata"; sha = "8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba" },
  @{ dir = "fast"; url = "https://github.com/tesseract-ocr/tessdata_fast/raw/main/eng.traineddata"; sha = "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2" }
)

function Assert-Sha256($path, $expected) {
  $actual = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
  if ($actual -ne $expected) { throw "checksum mismatch for $path`n expected $expected`n actual   $actual" }
}

$pdfiumDir = Join-Path $root "third_party/pdfium"
$dll = Join-Path $pdfiumDir "bin/pdfium.dll"
if (-not (Test-Path $dll)) {
  New-Item -ItemType Directory -Force $pdfiumDir | Out-Null
  $tgz = Join-Path $pdfiumDir "pdfium-win-x64.tgz"
  Invoke-WebRequest -Uri "https://github.com/bblanchon/pdfium-binaries/releases/download/$pdfiumTag/pdfium-win-x64.tgz" -OutFile $tgz
  tar -xzf $tgz -C $pdfiumDir
}
Assert-Sha256 $dll $pdfiumSha
Write-Host "pdfium ok: $dll"

foreach ($m in $models) {
  $dir = Join-Path $root "models/$($m.dir)"
  $file = Join-Path $dir "eng.traineddata"
  if (-not (Test-Path $file)) {
    New-Item -ItemType Directory -Force $dir | Out-Null
    Invoke-WebRequest -Uri $m.url -OutFile $file
  }
  Assert-Sha256 $file $m.sha
  Write-Host "model ok: $file"
}

# Flat lexicon folder in the layout the app bundles (see docs/dev-setup.md).
$lexSrc = Join-Path $root "fixtures/lexicons"
$lexDev = Join-Path $lexSrc "dev"
New-Item -ItemType Directory -Force $lexDev | Out-Null
foreach ($f in @("hunspell-en_GB-large/en_GB-large.aff", "hunspell-en_GB-large/en_GB-large.dic", "webster-1913-headwords.txt")) {
  Copy-Item (Join-Path $lexSrc $f) (Join-Path $lexDev (Split-Path $f -Leaf)) -Force
}
Write-Host "lexicons ok: $lexDev"

# Replay actual .emf review exports through Windows GDI+; no redraw or retouching.
# Generate light.emf/dark.emf with windows_emf_file_export_review first.
param([Parameter(Mandatory = $true)][string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
foreach ($name in @('light', 'dark')) {
    $metafile = [Drawing.Imaging.Metafile]::new("$Directory/$name.emf")
    try {
        $header = $metafile.GetMetafileHeader()
        $width = 800
        # Use the physical EMF frame rather than integer device pixel bounds.
        $bytes = [IO.File]::ReadAllBytes("$Directory/$name.emf")
        $frameWidth = [BitConverter]::ToInt32($bytes, 32) - [BitConverter]::ToInt32($bytes, 24)
        $frameHeight = [BitConverter]::ToInt32($bytes, 36) - [BitConverter]::ToInt32($bytes, 28)
        if ($frameWidth -le 0 -or $frameHeight -le 0) { throw 'Invalid EMF physical frame' }
        $height = [int][Math]::Ceiling($width * $frameHeight / $frameWidth)
        $bitmap = [Drawing.Bitmap]::new($width, $height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear([Drawing.Color]::Transparent)
            $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
            $graphics.DrawImage($metafile, [Drawing.Rectangle]::new(0, 0, $width, $height))
            $bitmap.Save("$Directory/$name-emf.png", [Drawing.Imaging.ImageFormat]::Png)
            if ($bitmap.GetPixel(2, 2).A -ne 255) {
                throw 'EMF file did not paint its canvas background'
            }
            [pscustomobject]@{
                name = $name
                emf_type = $header.Type.ToString()
                vector_dual = $header.IsEmfPlusDual()
                emf_bytes = (Get-Item "$Directory/$name.emf").Length
                rendered_width = $width
                rendered_height = $height
                background = $bitmap.GetPixel(2, 2).ToString()
            } | ConvertTo-Json | Set-Content "$Directory/$name-review.json"
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    } finally { $metafile.Dispose() }
}

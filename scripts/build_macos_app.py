"""Build a development bundle, or a relocatable bundle with --standalone."""
import argparse
from pathlib import Path
import plistlib
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--standalone', action='store_true', help='Bundle Python and RDKit with PyInstaller')
parser.add_argument('--release', action='store_true', help='Use an optimized Rust binary')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
profile = 'release' if args.release else 'debug'
subprocess.run(['cargo', 'build', '--locked'] + (['--release'] if args.release else []), cwd=root, check=True)
bundle = root / ('dist/Moruno.app' if args.standalone else f'target/{profile}/Moruno.app')
executable = bundle / 'Contents/MacOS/moruno'
executable.parent.mkdir(parents=True, exist_ok=True)
temporary_executable = executable.with_name('moruno.new')
shutil.copy2(root / f'target/{profile}/moruno', temporary_executable)
temporary_executable.replace(executable)
clipboard = executable.with_name('moruno-clipboard')
clipboard_temporary = clipboard.with_suffix('.new')
subprocess.run(['swiftc', '-O', str(root / 'native/macos/ClipboardSupport.swift'), str(root / 'native/macos/Clipboard.swift'), '-o', str(clipboard_temporary)], check=True)
clipboard_temporary.replace(clipboard)

print_bundle = bundle / 'Contents/Helpers/Moruno Print.app'
print_executable = print_bundle / 'Contents/MacOS/moruno-print'
print_executable.parent.mkdir(parents=True, exist_ok=True)
print_temporary = print_executable.with_suffix('.new')
subprocess.run(['swiftc', '-O', str(root / 'native/macos/PrintSupport.swift'), str(root / 'native/macos/Print.swift'), '-o', str(print_temporary)], check=True)
print_temporary.replace(print_executable)
with (print_bundle / 'Contents/Info.plist').open('wb') as stream:
    plistlib.dump({'CFBundleName': 'Moruno Print', 'CFBundleDisplayName': 'Moruno Print',
                  'CFBundleIdentifier': 'dev.moruno.print', 'CFBundleExecutable': 'moruno-print',
                  'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': '0.2.0',
                  'NSHighResolutionCapable': True, 'LSUIElement': True,
                  'LSMinimumSystemVersion': '12.0'}, stream)

if args.standalone:
    work = root / 'target/pyinstaller'
    work.mkdir(parents=True, exist_ok=True)
    subprocess.run([
        'uv', 'run', '--locked', '--group', 'packaging', 'pyinstaller',
        '--noconfirm', '--clean', '--onedir', '--name', 'moruno-engine',
        '--collect-all', 'rdkit', '--collect-all', 'numpy',
        '--add-data', str(root / 'engine/drawing_style.json') + ':.',
        '--distpath', str(work / 'dist'), '--workpath', str(work / 'build'),
        '--specpath', str(work), str(root / 'engine/worker.py'),
    ], cwd=root, check=True)
    destination = bundle / 'Contents/Resources/chemistry'
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(work / 'dist/moruno-engine', destination, symlinks=True)
    licenses = bundle / 'Contents/Resources/Licenses'
    licenses.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'metadata', '--format-version', '1', '--locked'], cwd=root,
                   check=True, stdout=(licenses / 'rust-dependencies.json').open('w'))
    # Wheel metadata includes RDKit, NumPy and their third-party notices.
    for metadata in (root / '.venv/lib').glob('python*/site-packages/*.dist-info'):
        shutil.copytree(metadata, licenses / metadata.name, dirs_exist_ok=True)
with (bundle / 'Contents/Info.plist').open('wb') as stream:
    plistlib.dump({'CFBundleName': 'Moruno', 'CFBundleDisplayName': 'Moruno',
                  'CFBundleIdentifier': 'dev.moruno.editor', 'CFBundleExecutable': 'moruno',
                  'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': '0.2.0',
                  'NSHighResolutionCapable': True, 'LSMinimumSystemVersion': '12.0'}, stream)
if args.standalone:
    subprocess.run(['codesign', '--force', '--deep', '--sign', '-', str(bundle)], check=True)
print(bundle)

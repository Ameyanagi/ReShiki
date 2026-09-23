; SourceDir, AppVersion, AppArchitecture, OutputDir and OutputName are set by installers.py.
[Setup]
AppId=dev.reshiki.editor
AppName=ReShiki
AppVersion={#AppVersion}
AppPublisher=ReShiki contributors
AppPublisherURL=https://reshiki.com/
AppSupportURL=https://reshiki.com/guide/install/
AppUpdatesURL=https://github.com/Ameyanagi/ReShiki/releases
DefaultDirName={localappdata}\Programs\ReShiki
DefaultGroupName=ReShiki
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
UninstallDisplayIcon={app}\reshiki.exe
SetupIconFile=..\..\assets\branding\reshiki.ico
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
#if AppArchitecture == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
MinVersion=10.0.22000
#else
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0
#endif
InfoBeforeFile=install.txt
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "fileassoc"; Description: "Open .rsk and older ReShiki drawings with ReShiki"

[InstallDelete]
; Retire only the former app-owned worker. Never remove user caches or drawings.
Type: filesandordirs; Name: "{app}\chemistry"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\ReShiki"; Filename: "{app}\reshiki.exe"
Name: "{userdesktop}\ReShiki"; Filename: "{app}\reshiki.exe"; Tasks: desktopicon

[Registry]
; Editable Office objects are independent of the optional drawing file associations.
Root: HKCU; Subkey: "Software\Classes\CLSID\{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}"; ValueType: string; ValueData: "ReShiki drawing"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\CLSID\{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}\LocalServer32"; ValueType: string; ValueData: """{app}\reshiki.exe"" --ole-server"
Root: HKCU; Subkey: "Software\Classes\CLSID\{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}\InprocHandler32"; ValueType: string; ValueData: "ole32.dll"
Root: HKCU; Subkey: "Software\Classes\CLSID\{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}\ProgID"; ValueType: string; ValueData: "ReShiki.EmbeddedDrawing.1"
Root: HKCU; Subkey: "Software\Classes\CLSID\{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}\Verb\0"; ValueType: string; ValueData: "Edit,0,2"
Root: HKCU; Subkey: "Software\Classes\CLSID\{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}\Verb\1"; ValueType: string; ValueData: "Open,0,2"
Root: HKCU; Subkey: "Software\Classes\ReShiki.EmbeddedDrawing.1"; ValueType: string; ValueData: "ReShiki drawing"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\ReShiki.EmbeddedDrawing.1\CLSID"; ValueType: string; ValueData: "{{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}"
Root: HKCU; Subkey: "Software\Classes\.rsk\OpenWithProgids"; ValueType: string; ValueName: "ReShiki.Drawing"; ValueData: ""; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.reshiki\OpenWithProgids"; ValueType: string; ValueName: "ReShiki.Drawing"; ValueData: ""; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\.moruno\OpenWithProgids"; ValueType: string; ValueName: "ReShiki.Drawing"; ValueData: ""; Flags: uninsdeletevalue; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\ReShiki.Drawing"; ValueType: string; ValueData: "ReShiki drawing"; Flags: uninsdeletekey; Tasks: fileassoc
Root: HKCU; Subkey: "Software\Classes\ReShiki.Drawing\shell\open\command"; ValueType: string; ValueData: """{app}\reshiki.exe"" --open ""%1"""; Tasks: fileassoc

[Run]
Filename: "{app}\reshiki.exe"; Description: "Open ReShiki"; Flags: nowait postinstall skipifsilent

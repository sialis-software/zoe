; The Windows installer, made by scripts/package-windows.ps1 with Inno Setup (https://jrsoftware.org).
; The script passes /DAppVersion=<version> and /DSourceDir=<release folder>.
[Setup]
AppId={{6B0E3D52-5C0F-4B8E-9E55-7A1D2C4F9A10}
AppName=Zoe
AppVersion={#AppVersion}
AppPublisher=Zoe
DefaultDirName={autopf}\Zoe
DefaultGroupName=Zoe
UninstallDisplayIcon={app}\Zoe.exe
SetupIconFile=..\icons\zoe.ico
OutputDir=..\..\target\dist
OutputBaseFilename=Zoe-{#AppVersion}-windows-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern

[Files]
Source: "{#SourceDir}\Zoe.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Zoe"; Filename: "{app}\Zoe.exe"
Name: "{autodesktop}\Zoe"; Filename: "{app}\Zoe.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\Zoe.exe"; Description: "Start Zoe"; Flags: nowait postinstall skipifsilent

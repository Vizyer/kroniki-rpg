#ifndef MyAppVersion
  #define MyAppVersion "0.8.1"
#endif
#ifndef BuildDir
  #define BuildDir "..\\build"
#endif

#define MyAppName "Kroniki RPG"
#define MyAppPublisher "Kroniki RPG"
#define MyAppExeName "KronikiLauncher.exe"

[Setup]
AppId={{A74DCB6E-E612-49CA-A3B3-69624B43A1A3}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\KronikiRPG
DefaultGroupName=Kroniki RPG
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#BuildDir}
OutputBaseFilename=KronikiRPG-Setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}
SetupLogging=yes

[Languages]
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Utwórz skrót na pulpicie"; GroupDescription: "Skróty:"; Flags: checkedonce

[Files]
Source: "{#BuildDir}\KronikiLauncher.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BuildDir}\launcher-config.json"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BuildDir}\app\*"; DestDir: "{app}\app"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Kroniki RPG"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\Kroniki RPG"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Uruchom Kroniki RPG"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}\.staging"
Type: filesandordirs; Name: "{app}\.rollback"

; Windows installer (Inno Setup 6). Installs for the current user only, so it
; needs no admin rights. Built by .github/workflows/release.yml:
;
;   iscc /DVersion=0.1.1 /DSource=C:\full\path\to\dist\rhythia-is-trash packaging\windows\rhythia-is-trash.iss
;
; Maps, settings and scores live next to rhythm.exe, so they stay on uninstall.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Source
  #define Source "..\..\dist\rhythia-is-trash"
#endif

[Setup]
AppId={{B3E1A7C2-5D84-4F0B-9A6E-2C7D1F8E4A93}
AppName=rhythia is trash
AppVersion={#Version}
AppVerName=rhythia is trash {#Version}
AppPublisher=cj
AppCopyright=cj, MIT
VersionInfoDescription=rhythia is trash installer
AppSupportURL=https://github.com/sjdsjadjajdjsa/rhythia-is-trash
AppUpdatesURL=https://github.com/sjdsjadjajdjsa/rhythia-is-trash/releases
DefaultDirName={userpf}\rhythia is trash
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile={#Source}\LICENSE
SetupIconFile=rhythia-is-trash.ico
UninstallDisplayIcon={app}\rhythm.exe
UninstallDisplayName=rhythia is trash
OutputDir=..\..
OutputBaseFilename=rhythia-is-trash-setup-x86_64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "{#Source}\rhythm.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\FONTS-NOTICE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
Name: "{app}\maps"

[Icons]
Name: "{userprograms}\rhythia is trash"; Filename: "{app}\rhythm.exe"; WorkingDir: "{app}"; Comment: "Aim rhythm game"
Name: "{userdesktop}\rhythia is trash"; Filename: "{app}\rhythm.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\rhythm.exe"; Description: "Play now"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

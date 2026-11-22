; OVERLIME Remote Play - Installer (Inno Setup 6)
; Bauen: installer\build.ps1   (erzeugt installer\Output\OVERLIME-Setup-<Version>.exe)

#ifndef MyAppVersion
  #define MyAppVersion "0.13"
#endif
#ifndef ExeDir
  #define ExeDir "..\..\release-" + MyAppVersion
#endif
#define MyAppExe "overlime.exe"

[Setup]
AppId={{7C1B1F52-3E6A-4E5C-9B0A-5D7D8E2A4F61}
AppName=OVERLIME Remote Play
AppVersion={#MyAppVersion}
AppVerName=OVERLIME Remote Play {#MyAppVersion}
AppPublisher=Overtime09991
AppPublisherURL=https://github.com/Overtime09991/overlime
AppSupportURL=https://github.com/Overtime09991/overlime/issues
AppUpdatesURL=https://github.com/Overtime09991/overlime/releases
VersionInfoVersion={#MyAppVersion}.0.0
DefaultDirName={autopf}\OVERLIME
DefaultGroupName=OVERLIME
DisableProgramGroupPage=yes
; Ohne Administrator-Rechte, pro Benutzer. Dadurch kann sich die App selbst aktualisieren.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=Output
OutputBaseFilename=OVERLIME-Setup-{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
WizardImageFile=assets\wizard.bmp
WizardSmallImageFile=assets\wizard-small.bmp
SetupIconFile=assets\overlime.ico
UninstallDisplayIcon={app}\overlime.ico
UninstallDisplayName=OVERLIME Remote Play
CloseApplications=yes
RestartApplications=no
LicenseFile=..\LICENSE

[Languages]
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "Symbol auf dem &Desktop anlegen"; GroupDescription: "Zusätzliche Optionen:"; Flags: unchecked
Name: "autostart"; Description: "OVERLIME beim &Anmelden an Windows automatisch starten"; GroupDescription: "Zusätzliche Optionen:"; Flags: unchecked

[Files]
Source: "{#ExeDir}\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
; Windows-Laufzeitdatei, damit die App auch auf frischen PCs startet (App-lokal erlaubt)
Source: "{sys}\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion external
Source: "assets\overlime.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OVERLIME"; Filename: "{app}\{#MyAppExe}"; IconFilename: "{app}\overlime.ico"
Name: "{autodesktop}\OVERLIME"; Filename: "{app}\{#MyAppExe}"; IconFilename: "{app}\overlime.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "OVERLIME"; ValueData: """{app}\{#MyAppExe}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "OVERLIME jetzt starten"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Reste des automatischen Updates
Type: files; Name: "{app}\overlime.old"

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and (not UninstallSilent) then
  begin
    if MsgBox('Sollen auch deine Einstellungen, die PlayStation-Anmeldung und die gekoppelten Konsolen gelöscht werden?'#13#10#13#10'Wenn du OVERLIME später wieder installierst, musst du dann alles neu einrichten.',
      mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DelTree(ExpandConstant('{userappdata}\overlime'), True, True, True);
  end;
end;

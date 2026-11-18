; VT Tray installer (Inno Setup 6). Per-user: no administrator rights, nothing outside the
; user profile. Build:  ISCC.exe /DAppVersion=1.0.0 installer\vtray.iss
; Expects target\release\VTray.exe from `cargo build --release`.

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif

[Setup]
; Never change AppId: Windows uses it to recognise upgrades and the Settings > Apps entry.
AppId={{306A0086-1942-4B5D-92A4-7F6A8F10E873}
AppName=VT Tray
AppVersion={#AppVersion}
AppVerName=VT Tray {#AppVersion}
AppPublisher=abusayed.dev
AppPublisherURL=https://abusayed.dev
AppSupportURL=https://vtray.sayed.app
AppUpdatesURL=https://github.com/abusayed0206/vtray/releases
AppCopyright=Copyright (c) 2026 abusayed.dev
VersionInfoVersion={#AppVersion}
VersionInfoCompany=abusayed.dev
VersionInfoDescription=VT Tray Setup
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\VT Tray
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes
OutputDir=..\dist
OutputBaseFilename=VTray-Setup-{#AppVersion}
SetupIconFile=..\resources\vt-tray.ico
UninstallDisplayIcon={app}\VTray.exe
UninstallDisplayName=VT Tray
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; The running tray holds this mutex; setup asks the user to close it before updating.
AppMutex=Local\VTTray.SingleInstance

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Start VT Tray when I sign in to Windows"; GroupDescription: "Startup:"
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "..\target\release\VTray.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\VT Tray"; Filename: "{app}\VTray.exe"; Comment: "Check files with VirusTotal"
Name: "{userdesktop}\VT Tray"; Filename: "{app}\VTray.exe"; Comment: "Check files with VirusTotal"; Tasks: desktopicon

[Registry]
; Same Run value the app's own "Start with Windows" setting uses, so the two stay in sync.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "VT Tray"; ValueData: """{app}\VTray.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\VTray.exe"; Description: "Start VT Tray now"; Flags: nowait postinstall skipifsilent

[Code]
const
  WM_CLOSE = $0010;

{ Ask a running tray to exit (like its Exit menu item) so updates and uninstall are not
  blocked by AppMutex. Waits up to 5 seconds for its hidden window to disappear. }
procedure CloseRunningTray;
var
  Wnd: HWND;
  Waited: Integer;
begin
  Wnd := FindWindowByClassName('VTTrayHiddenWindow');
  if Wnd = 0 then
    Exit;
  PostMessage(Wnd, WM_CLOSE, 0, 0);
  Waited := 0;
  while (FindWindowByClassName('VTTrayHiddenWindow') <> 0) and (Waited < 5000) do
  begin
    Sleep(100);
    Waited := Waited + 100;
  end;
  { Give the process a moment to release its mutex and files. }
  Sleep(500);
end;

function InitializeSetup(): Boolean;
begin
  CloseRunningTray;
  Result := True;
end;

function InitializeUninstall(): Boolean;
begin
  CloseRunningTray;
  Result := True;
end;

[UninstallRun]
; Closes a running tray and removes what the app created itself: startup and Explorer
; entries, the saved API key, settings, cache, and logs.
Filename: "{app}\VTray.exe"; Parameters: "--cleanup"; RunOnceId: "VTrayCleanup"; Flags: runhidden waituntilterminated

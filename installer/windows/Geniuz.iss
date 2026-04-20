; Geniuz Free for Windows — Inno Setup installer script
;
; Per-user install only. Memory is per-user by design (your memories aren't
; your colleague's memories), and per-user install sidesteps the all-users
; ambiguity where the installer can't pre-create per-user data dirs for
; users who haven't logged in yet.
;
; The user picks the data directory (memory.db location) during install.
; Default: %USERPROFILE%\.geniuz. The installer creates the chosen dir in
; user context (no sandbox restriction) and grants ALL APPLICATION PACKAGES
; write access — so when sandboxed Claude Desktop later launches geniuz as
; a child process, it can read/write the database without a sandbox-denied
; mkdir failure.

#define MyAppName "Geniuz"
#define MyAppVersion "1.1.6"
#define MyAppPublisher "Managed Ventures LLC"
#define MyAppURL "https://geniuz.life"
#define MyAppExeName "geniuz.exe"
#define MyAppTrayExeName "geniuz-tray.exe"
#define MyAppDescription "Your AI remembers now."

[Setup]
AppId={{B0EA9F1E-5C2C-4C1B-9F4A-7D3E1B2C9A7E}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
AppComments={#MyAppDescription}
; Version resource embedded in Setup.exe itself — drives Windows Explorer's
; right-click → Properties → Details and Programs & Features' version column.
; Separate from AppVersion which only affects the wizard's displayed strings.
VersionInfoVersion={#MyAppVersion}
VersionInfoProductVersion={#MyAppVersion}
VersionInfoProductName={#MyAppName}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} {#MyAppVersion} Setup
VersionInfoCopyright=Copyright (C) 2026 {#MyAppPublisher}
DefaultDirName={localappdata}\Programs\Geniuz
DefaultGroupName=Geniuz
DisableProgramGroupPage=yes
; Per-user install only — no admin elevation, no all-users option.
PrivilegesRequired=lowest
OutputDir=output
OutputBaseFilename=Geniuz-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 11 only. Win10 build 22000 is Win11 RTM — anything lower is Win10
; or earlier. Hard-blocks at install-start so the user gets a clear
; "requires Windows 11" message before any files touch disk. Win10 support
; is archived as a commercial-customer recipe at installer/windows/saved-for-later/README.md.
MinVersion=10.0.22000
UninstallDisplayName={#MyAppName}
UninstallDisplayIcon={app}\Geniuz.ico
SetupIconFile=Geniuz.ico
; Branded wizard page images (Geniuz logo on white).
; WizardImageFile is shown on the welcome + finish pages (164x314 portrait).
; WizardSmallImageFile is shown in the top-right of other pages (55x58).
WizardImageFile=WizardImage.bmp
WizardSmallImageFile=WizardSmallImage.bmp

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "geniuz.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "geniuz-embed.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "geniuz-tray.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "Geniuz.ico"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
; Create the user-chosen data directory in user context (no sandbox issue).
Name: "{code:GetDataDir}"

[Registry]
; Add install dir to user PATH (only if not already present).
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
  ValueData: "{olddata};{app}"; \
  Check: NeedsAddPath('{app}')
; Persist the user's data directory choice as an environment variable.
; This lets the CLI (from any future shell) and external tools discover
; where memories live without parsing the Claude Desktop config.
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "GENIUZ_HOME"; \
  ValueData: "{code:GetDataDir}"
; Autostart the tray at login so Geniuz is an ambient presence — visible
; immediately when the user signs in, without having to launch anything.
; HKCU so it's per-user and no admin elevation needed.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
  ValueType: string; ValueName: "Geniuz"; \
  ValueData: """{app}\{#MyAppTrayExeName}"""; \
  Flags: uninsdeletevalue

[Icons]
; Start Menu entry — opens the tray if it's not running, otherwise harmless.
; Users who quit the tray can re-launch from here.
Name: "{userprograms}\Geniuz"; Filename: "{app}\{#MyAppTrayExeName}"; \
  IconFilename: "{app}\Geniuz.ico"; Comment: "{#MyAppDescription}"

[Run]
; Grant ALL APPLICATION PACKAGES (S-1-15-2-1) read+write on the data dir.
; Without this, sandboxed Claude Desktop can launch geniuz.exe but the
; child can't access the database file → "Geniuz Disconnected" in Claude.
; (OI)(CI)F = object inherit + container inherit + full control on contents.
Filename: "{sys}\icacls.exe"; \
  Parameters: """{code:GetDataDir}"" /grant ""*S-1-15-2-1:(OI)(CI)M"""; \
  StatusMsg: "Granting sandboxed apps access to memory folder..."; \
  Flags: runhidden

; Wire Claude Desktop MCP config. Pass GENIUZ_HOME so the entry includes
; the env block — important on Windows because sandboxed Claude doesn't
; inherit the user's HKCU environment; the path has to be embedded in the
; MCP config itself.
Filename: "{app}\{#MyAppExeName}"; \
  Parameters: "mcp install --env GENIUZ_HOME=""{code:GetDataDir}"""; \
  StatusMsg: "Configuring Claude Desktop integration..."; \
  Flags: runhidden

; Launch the tray immediately so the user sees Geniuz appear as soon as
; the installer finishes — not on next login. `nowait` because the tray
; runs continuously; we don't want to block the installer.
Filename: "{app}\{#MyAppTrayExeName}"; \
  Description: "Launch Geniuz now"; \
  Flags: postinstall nowait skipifsilent

[Code]
var
  DataDirPage: TInputDirWizardPage;

procedure InitializeWizard;
begin
  DataDirPage := CreateInputDirPage(
    wpSelectDir,
    'Memory location',
    'Where should Geniuz keep your memories?',
    'Geniuz keeps your memories — and the embedding model that searches them — in this folder.' + #13#10 + #13#10 +
    'The default works for most people. You can change it to any folder you have rights to write — for example a different drive, or a synced folder if you want memories across machines.' + #13#10 + #13#10 +
    'You can change this later by editing the GENIUZ_HOME environment variable.',
    False, ''
  );
  DataDirPage.Add('');
  // Inno Setup has no {userprofile} constant; use the env-var expansion syntax.
  DataDirPage.Values[0] := ExpandConstant('{%USERPROFILE}\.geniuz');
end;

function GetDataDir(Param: string): string;
begin
  Result := DataDirPage.Values[0];
end;

function NeedsAddPath(Param: string): Boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  // Treat case-insensitively, match exact entry boundaries
  Result := Pos(';' + LowerCase(Param) + ';', ';' + LowerCase(OrigPath) + ';') = 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  OrigPath: string;
  AppDir: string;
  NewPath: string;
  ResultCode: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    // Kill the running tray — otherwise we can't delete the running .exe
    // and the user has a lingering process after uninstall completes.
    Exec('taskkill.exe', '/F /IM geniuz-tray.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    // PATH cleanup
    if RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
    begin
      NewPath := OrigPath;
      StringChangeEx(NewPath, ';' + AppDir, '', True);
      StringChangeEx(NewPath, AppDir + ';', '', True);
      StringChangeEx(NewPath, AppDir, '', True);
      RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', NewPath);
    end;
    // Remove the GENIUZ_HOME pointer. We do NOT delete the data directory
    // itself — user memories survive uninstall by design.
    RegDeleteValue(HKEY_CURRENT_USER, 'Environment', 'GENIUZ_HOME');
  end;
end;

; Geniuz Free for Windows — Inno Setup installer script
; Per-user install (no admin), matches the philosophy of "your AI on your machine"

#define MyAppName "Geniuz"
#define MyAppVersion "1.0.1"
#define MyAppPublisher "Managed Ventures LLC"
#define MyAppURL "https://geniuz.life"
#define MyAppExeName "geniuz.exe"
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
DefaultDirName={localappdata}\Programs\Geniuz
DefaultGroupName=Geniuz
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=output
OutputBaseFilename=Geniuz-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayName={#MyAppName}
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "geniuz.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "geniuz-embed.exe"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
; Add install dir to user PATH (only if not already present)
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
  ValueData: "{olddata};{app}"; \
  Check: NeedsAddPath('{app}')

[Run]
; Wire Claude Desktop MCP config (runs in user context — writes to %APPDATA%)
Filename: "{app}\{#MyAppExeName}"; Parameters: "mcp install"; \
  StatusMsg: "Configuring Claude Desktop integration..."; \
  Flags: runhidden

[Code]
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
begin
  if CurUninstallStep = usUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    if RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
    begin
      // Remove ;AppDir from PATH (idempotent, case-insensitive).
      // StringChangeEx mutates the var-string and returns Integer (count).
      NewPath := OrigPath;
      StringChangeEx(NewPath, ';' + AppDir, '', True);
      StringChangeEx(NewPath, AppDir + ';', '', True);
      StringChangeEx(NewPath, AppDir, '', True);
      RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', NewPath);
    end;
  end;
end;

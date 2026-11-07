#ifndef AppVersion
  #error AppVersion must match Cargo.toml
#endif

[Setup]
AppId=EasyMediaCompressRust
AppName=Easy Media Compress
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\EasyMediaCompress
DefaultGroupName=Easy Media Compress
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
ChangesAssociations=yes
UninstallDisplayIcon={app}\EasyMediaCompress.exe
SetupIconFile=..\icon\icon.ico
OutputDir=..\dist
OutputBaseFilename=EasyMediaCompress-Setup-x64
Compression=lzma2/fast
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "explorer"; Description: "{cm:ExplorerTask}"
Name: "cli"; Description: "{cm:CliTask}"

[CustomMessages]
english.ExplorerTask=Add an Explorer context menu for media and folders
russian.ExplorerTask=Добавить действие в меню Проводника для медиа и папок
english.CliTask=Enable the compress command in new terminals
russian.CliTask=Добавить команду compress в PATH пользователя

[Files]
Source: "..\dist\EasyMediaCompress\*"; DestDir: "{app}"; Excludes: "win_install.ps1"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Easy Media Compress"; Filename: "{app}\EasyMediaCompress.exe"; WorkingDir: "{app}"

[Registry]
Root: HKCU; Subkey: "Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressRust"; ValueType: string; ValueData: "Easy Media Compress"; Tasks: explorer; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressRust"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\EasyMediaCompress.exe"",0"; Tasks: explorer
Root: HKCU; Subkey: "Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressRust"; ValueType: string; ValueName: "AppliesTo"; ValueData: "System.ItemType:=Directory OR System.FileExtension:=.mp4 OR System.FileExtension:=.mkv OR System.FileExtension:=.avi OR System.FileExtension:=.mov OR System.FileExtension:=.flv OR System.FileExtension:=.wmv OR System.FileExtension:=.webm OR System.FileExtension:=.jpg OR System.FileExtension:=.jpeg OR System.FileExtension:=.jfif OR System.FileExtension:=.png OR System.FileExtension:=.bmp OR System.FileExtension:=.tif OR System.FileExtension:=.tiff OR System.FileExtension:=.webp OR System.FileExtension:=.heic OR System.FileExtension:=.heif OR System.FileExtension:=.avif"; Tasks: explorer
Root: HKCU; Subkey: "Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressRust"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Single"; Tasks: explorer
Root: HKCU; Subkey: "Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressRust\command"; ValueType: string; ValueData: """{app}\EasyMediaCompress.exe"" --gui ""%1"""; Tasks: explorer

[Run]
Filename: "{app}\EasyMediaCompress.exe"; Description: "{cm:LaunchProgram,Easy Media Compress}"; Flags: nowait postinstall skipifsilent; Check: LaunchAfterSetup

[Code]
function LaunchAfterSetup: Boolean;
var
  I: Integer;
begin
  Result := True;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/LEGACYBRIDGE') = 0 then Result := False;
end;

procedure UpdateUserPath(Add: Boolean);
var
  OldPath, NewPath, AppDir: String;
  Entries: TArrayOfString;
  I: Integer;
  Found: Boolean;
begin
  OldPath := '';
  RegQueryStringValue(HKCU, 'Environment', 'Path', OldPath);
  AppDir := ExpandConstant('{app}');
  Entries := StringSplit(OldPath, [';'], stExcludeEmpty);
  NewPath := '';
  Found := False;
  for I := 0 to GetArrayLength(Entries) - 1 do
  begin
    if CompareText(Trim(Entries[I]), AppDir) = 0 then
      Found := True
    else
    begin
      if NewPath <> '' then NewPath := NewPath + ';';
      NewPath := NewPath + Entries[I];
    end;
  end;
  if Add = Found then exit;
  if Add then
  begin
    NewPath := OldPath;
    if (NewPath <> '') and (NewPath[Length(NewPath)] <> ';') then
      NewPath := NewPath + ';';
    NewPath := NewPath + AppDir;
  end;
  if not RegWriteExpandStringValue(HKCU, 'Environment', 'Path', NewPath) then
    RaiseException('Could not update the user PATH.');
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  LegacyCommand: String;
begin
  if CurStep = ssPostInstall then
  begin
    UpdateUserPath(WizardIsTaskSelected('cli'));
    if not WizardIsTaskSelected('explorer') then
      RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\AllFileSystemObjects\shell\EasyMediaCompressRust');
    if RegQueryStringValue(HKCU, 'Software\Classes\AllFileSystemObjects\shell\EasyMediaCompress\command', '', LegacyCommand) and
       (Pos('cmd.exe /c ""', Lowercase(LegacyCommand)) = 1) and
       (Pos('\compress.bat" %v"', Lowercase(LegacyCommand)) > 0) then
      RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\AllFileSystemObjects\shell\EasyMediaCompress');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then UpdateUserPath(False);
end;

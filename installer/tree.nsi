; Tree — per-user installer. No admin prompt, no services, no registry sprawl.
Unicode true
Name "Tree — download manager"
OutFile "..\dist\Tree_0.6.0_setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Tree"
InstallDirRegKey HKCU "Software\Tree" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Tree (required)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "..\dist\tree.exe"
  File "..\LICENSE"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Tree"
  CreateShortcut "$SMPROGRAMS\Tree\Tree.lnk" "$INSTDIR\tree.exe"
  CreateShortcut "$SMPROGRAMS\Tree\Uninstall Tree.lnk" "$INSTDIR\uninstall.exe"
  CreateShortcut "$DESKTOP\Tree.lnk" "$INSTDIR\tree.exe"

  WriteRegStr HKCU "Software\Tree" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "DisplayName" "Tree — download manager"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "DisplayVersion" "0.6.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "DisplayIcon" "$INSTDIR\tree.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "Publisher" "R"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\tree.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Tree\Tree.lnk"
  Delete "$SMPROGRAMS\Tree\Uninstall Tree.lnk"
  RMDir "$SMPROGRAMS\Tree"
  Delete "$DESKTOP\Tree.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tree"
  DeleteRegKey HKCU "Software\Tree"
  ; %APPDATA%\Tree (state.json + settings) is left alone on purpose.
SectionEnd

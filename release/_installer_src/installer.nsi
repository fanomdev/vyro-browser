Name "VYRO Browser"
OutFile "vyro-browser-setup.exe"
InstallDir "$PROGRAMFILES\VYRO Browser"
InstallDirRegKey HKCU "Software\VYRO Browser" "InstallDir"
RequestExecutionLevel user
SetCompressor LZMA
ShowInstDetails show

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "VYRO Browser"
  SetOutPath "$INSTDIR"
  File "vyro-browser.exe"
  File "WebView2Loader.dll"
  ; Portable marker: browser state is stored in $INSTDIR\vyro-data,
  ; so the install works on any drive without writing to the user profile.
  File "portable"
  File "README.txt"
  WriteRegStr HKCU "Software\VYRO Browser" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$DESKTOP\VYRO Browser.lnk" "$INSTDIR\vyro-browser.exe"
  CreateDirectory "$SMPROGRAMS\VYRO Browser"
  CreateShortCut "$SMPROGRAMS\VYRO Browser\VYRO Browser.lnk" "$INSTDIR\vyro-browser.exe"
  CreateShortCut "$SMPROGRAMS\VYRO Browser\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\vyro-browser.exe"
  Delete "$INSTDIR\WebView2Loader.dll"
  Delete "$INSTDIR\portable"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$DESKTOP\VYRO Browser.lnk"
  Delete "$SMPROGRAMS\VYRO Browser\VYRO Browser.lnk"
  Delete "$SMPROGRAMS\VYRO Browser\Uninstall.lnk"
  RMDir "$SMPROGRAMS\VYRO Browser"
  DeleteRegKey HKCU "Software\VYRO Browser"
  ; vyro-data is intentionally left in place so user data is not destroyed.
SectionEnd

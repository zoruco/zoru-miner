!ifndef STAGING
  !error "STAGING directory is required"
!endif
!ifndef OUTPUT
  !error "OUTPUT installer path is required"
!endif

Unicode true
Name "ZORU Miner"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\ZORU Miner"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "ZORU Miner" MainSection
    SetOutPath "$INSTDIR"
    File /r "${STAGING}\*"
    CreateDirectory "$SMPROGRAMS\ZORU Miner"
    CreateShortCut "$SMPROGRAMS\ZORU Miner\ZORU Miner.lnk" "$INSTDIR\bin\zoru-miner-gui.exe"
    CreateShortCut "$SMPROGRAMS\ZORU Miner\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZORUMiner" "DisplayName" "ZORU Miner"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZORUMiner" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZORUMiner" "DisplayVersion" "0.1.0"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZORUMiner" "Publisher" "ZORU Labs"
SectionEnd

Section "Uninstall"
    Delete "$SMPROGRAMS\ZORU Miner\ZORU Miner.lnk"
    Delete "$SMPROGRAMS\ZORU Miner\Uninstall.lnk"
    RMDir "$SMPROGRAMS\ZORU Miner"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZORUMiner"
    RMDir /r "$INSTDIR"
SectionEnd

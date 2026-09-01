!ifndef PRODUCT_VERSION
!error "PRODUCT_VERSION must be defined by the packaging script"
!endif

!ifndef PRODUCT_ARCH
!error "PRODUCT_ARCH must be defined by the packaging script"
!endif

!if "${PRODUCT_ARCH}" != "x86_64"
!if "${PRODUCT_ARCH}" != "aarch64"
!error "PRODUCT_ARCH must be x86_64 or aarch64"
!endif
!endif

!ifndef SOURCE_DIR
!error "SOURCE_DIR must be defined by the packaging script"
!endif

!ifndef OUTPUT_FILE
!error "OUTPUT_FILE must be defined by the packaging script"
!endif

!ifndef UNINSTALL_INCLUDE
!error "UNINSTALL_INCLUDE must be defined by the packaging script"
!endif

!define PRODUCT_NAME "NEO Emacs"
!define PRODUCT_REGISTRATION_NAME "${PRODUCT_NAME} (User)"
!define PRODUCT_PUBLISHER "eval-exec"
!define PRODUCT_WEB_SITE "https://github.com/eval-exec/neomacs"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_REGISTRATION_NAME}"
!define NEOMACS_APP_PATH_KEY "Software\Microsoft\Windows\CurrentVersion\App Paths\neomacs.exe"
!define NEOMACSCLIENT_APP_PATH_KEY "Software\Microsoft\Windows\CurrentVersion\App Paths\neomacsclient.exe"

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\${PRODUCT_NAME}"
InstallDirRegKey HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation"
ShowInstDetails show
ShowUnInstDetails show
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

!macro RemoveOwnedAppPath KEY EXECUTABLE
  ReadRegStr $0 HKCU "${KEY}" ""
  ${If} $0 == "$INSTDIR\bin\${EXECUTABLE}"
    DeleteRegValue HKCU "${KEY}" ""
    ReadRegStr $1 HKCU "${KEY}" "Path"
    ${If} $1 == "$INSTDIR\bin"
      DeleteRegValue HKCU "${KEY}" "Path"
    ${EndIf}
    DeleteRegKey /ifempty HKCU "${KEY}"
  ${EndIf}
!macroend

Function RemovePreviousUserInstallation
  ReadRegStr $R0 HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation"
  ${If} $R0 != ""
  ${AndIf} ${FileExists} "$R0\uninstall.exe"
    DetailPrint "Removing the previous ${PRODUCT_NAME} user installation..."
    ExecWait '"$R0\uninstall.exe" /S _?=$R0' $R1
    ${If} $R1 != 0
      MessageBox MB_OK|MB_ICONSTOP \
        "The previous ${PRODUCT_NAME} installation could not be removed (exit code $R1)."
      Abort
    ${EndIf}

    ; _?= runs the uninstaller IN PLACE, and an in-place uninstaller cannot
    ; delete itself - its own image is open - so the old uninstall.exe is still
    ; here.  WriteUninstaller must overwrite it later, and on windows-11-arm
    ; that overwrite is silently LOST: the installed uninstaller stays
    ; byte-identical to the previous version's, so a later uninstall runs the
    ; OLD file list.  It deletes what both versions share and strands
    ; everything this version added.  Measured: aarch64 fails, x86_64 passes.
    ;
    ; Remove it explicitly, retrying while the just-exited image is released.
    StrCpy $R2 0
    remove_stale_uninstaller:
    ${If} ${FileExists} "$R0\uninstall.exe"
      Delete "$R0\uninstall.exe"
      ${If} ${FileExists} "$R0\uninstall.exe"
      ${AndIf} $R2 < 100
        IntOp $R2 $R2 + 1
        Sleep 100
        Goto remove_stale_uninstaller
      ${EndIf}
    ${EndIf}
    ${If} ${FileExists} "$R0\uninstall.exe"
      MessageBox MB_OK|MB_ICONSTOP \
        "The previous ${PRODUCT_NAME} uninstaller could not be replaced."
      Abort
    ${EndIf}
  ${EndIf}
FunctionEnd

Function .onInit
!if "${PRODUCT_ARCH}" == "aarch64"
  ${IfNot} ${IsNativeARM64}
    MessageBox MB_OK "${PRODUCT_NAME} for ARM64 requires Windows on ARM64."
    Abort
  ${EndIf}
!else
  ${IfNot} ${IsNativeAMD64}
    MessageBox MB_OK "${PRODUCT_NAME} for x86_64 requires x86_64 Windows."
    Abort
  ${EndIf}
!endif
  SetRegView 64
  SetShellVarContext current
FunctionEnd

Function un.onInit
  SetRegView 64
  SetShellVarContext current
FunctionEnd

Section "!${PRODUCT_NAME}" SEC_MAIN
  Call RemovePreviousUserInstallation
  SetRegView 64
  SetOutPath "$INSTDIR"
  SetOverwrite on

  File /r "${SOURCE_DIR}\*.*"

  WriteRegStr HKCU "${NEOMACS_APP_PATH_KEY}" "" "$INSTDIR\bin\neomacs.exe"
  WriteRegStr HKCU "${NEOMACS_APP_PATH_KEY}" "Path" "$INSTDIR\bin"
  WriteRegStr HKCU "${NEOMACSCLIENT_APP_PATH_KEY}" "" "$INSTDIR\bin\neomacsclient.exe"
  WriteRegStr HKCU "${NEOMACSCLIENT_APP_PATH_KEY}" "Path" "$INSTDIR\bin"

  ; A lost WriteUninstaller is silent - it only sets the error flag - and what
  ; it leaves behind is a working installation with the WRONG uninstaller, so
  ; the damage only appears when someone uninstalls.  Fail here instead.
  ClearErrors
  WriteUninstaller "$INSTDIR\uninstall.exe"
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONSTOP \
      "${PRODUCT_NAME} could not write its uninstaller to $INSTDIR."
    Abort
  ${EndIf}

  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\bin\neomacs.exe"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayName" "${PRODUCT_REGISTRATION_NAME}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\bin\neomacs.exe,0"
  WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

Section Uninstall
  !insertmacro RemoveOwnedAppPath "${NEOMACS_APP_PATH_KEY}" "neomacs.exe"
  !insertmacro RemoveOwnedAppPath "${NEOMACSCLIENT_APP_PATH_KEY}" "neomacsclient.exe"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  !include "${UNINSTALL_INCLUDE}"

  DeleteRegKey HKCU "${PRODUCT_UNINST_KEY}"
SectionEnd

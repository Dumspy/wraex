; Wraex's Windows installer: an assisted (not one-click) per-user NSIS 3 setup of one
; packaged release directory (`tools/package/package.py --installer`, which is what
; `.github/workflows/release.yml` runs on its Linux runner):
;
;     makensis -DVERSION=0.1.0 \
;              -DSTAGE_DIR=dist/wraex-0.1.0-x86_64-pc-windows-msvc \
;              -DICON_FILE=/tmp/wraex.ico \
;              -DOUT_FILE=dist/wraex-0.1.0-x86_64-pc-windows-msvc-setup.exe \
;              tools/package/installer.nsi
;
; `-D` (not `/D`) because makensis on POSIX reads `/` as a path; values may contain
; spaces, but must not be quoted (NSIS keeps the quotes as characters).
;
; It matches upstream's electron-builder setup (`ref/apt/main/electron-builder.yml`:
; `nsis: {oneClick: false, allowToChangeInstallationDirectory: true}`):
;
; * assisted: welcome, install directory, components, progress, finish, with the
;   directory page enabled and the app offered to be started afterwards;
; * per-user, like electron-builder's default `perMachine: false`: the default is
;   `%LOCALAPPDATA%\Programs\Wraex`, no elevation (`RequestExecutionLevel user`), and
;   the Start menu and desktop entries are the current user's;
; * the packaged directory is installed as-is. `wraex_core::assets` looks for `assets/`
;   next to the executable, so `wraex.exe`, `assets/` and `licences/` keep that layout;
; * uninstalling removes the program files but not the user's settings and log in
;   `%APPDATA%\Wraex` (config.json, wraex.log), so a reinstall keeps them.
;
; Upgrades and the future self-updater (issue #3) rely on three things:
;
; * `InstallDirRegKey` below: a newer setup run with `/S` reinstalls over the previous
;   directory without any UI, because the installer starts from the `InstallLocation`
;   value of its own Add/Remove Programs entry before the (unshown) directory page.
;   An explicit `/D=<dir>` still wins over the registry. `Uninstall.exe` is rewritten in
;   place and the entry is updated, so an updater is a plain silent install and a
;   restart;
; * the fixed layout: `wraex.exe`, `assets/`, `licences/`, `wraex.ico`, `Uninstall.exe`
;   in `$INSTDIR`. An updater must keep writing those names; the old uninstaller is
;   replaced by the new one, so its file list does not need to match. Files an older
;   version shipped and a newer one does not stay until uninstall, which is harmless:
;   the app reads its assets by path;
; * the desktop-shortcut choice is remembered in `HKCU\Software\Wraex`, because a
;   silent upgrade has no components page to ask again.

Unicode true
; electron-builder's NSIS target compresses the same way.
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "Sections.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!ifndef STAGE_DIR
  !error "pass -DSTAGE_DIR=<dist/wraex-<version>-<target>> (tools/package/package.py --installer does)"
!endif
!ifndef VERSION
  !error "pass -DVERSION=<version>"
!endif
!ifndef ICON_FILE
  !error "pass -DICON_FILE=<multi-size .ico> (tools/package/package.py --installer converts assets/poe1/icon.png)"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "${__FILEDIR__}\wraex-setup.exe"
!endif

!define APP_NAME "Wraex"
!define APP_EXE "wraex.exe"
!define APP_ICON "wraex.ico"
!define APP_PUBLISHER "Wraex contributors"
!define REG_UNINSTALL "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}"
!define REG_SETTINGS "Software\${APP_NAME}"

Name "${APP_NAME} ${VERSION}"
BrandingText "${APP_NAME} ${VERSION}"
OutFile "${OUT_FILE}"
; electron-builder's per-user default: %LOCALAPPDATA%\Programs\<product name>.
InstallDir "$LOCALAPPDATA\Programs\${APP_NAME}"
; Read by the directory page and by a silent install; kept in sync by the sections
; below. This is what makes `setup.exe /S` an in-place upgrade.
InstallDirRegKey HKCU "${REG_UNINSTALL}" "InstallLocation"
; No administrator rights: the install, its registry keys and its shortcuts are the
; current user's, which also means no UAC prompt and no HKLM writes.
RequestExecutionLevel user

VIProductVersion "${VI_VERSION}"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP_NAME} setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT; see licences\NOTICES.md"

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"

!define MUI_WELCOMEPAGE_TITLE "Welcome to the ${APP_NAME} ${VERSION} setup"
!define MUI_WELCOMEPAGE_TEXT "This installs ${APP_NAME} ${VERSION} for your user account only, so no administrator rights are needed.$\r$\n$\r$\n${APP_NAME} is installed with the game data, images and licences it reads at runtime. Your settings and log in %APPDATA%\${APP_NAME} are kept when you uninstall.$\r$\n$\r$\nClick Next to continue."

!define MUI_DIRECTORYPAGE_TEXT_TOP "${APP_NAME} is installed for the current user. The default is a per-user directory under %LOCALAPPDATA%; choose another one if you prefer."

!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Run ${APP_NAME}"

!define MUI_UNCONFIRMPAGE_TEXT_TOP "This removes ${APP_NAME} ${VERSION} from the directory below. Your settings and log in %APPDATA%\${APP_NAME} are kept."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

; English only, like the app for now (PLAN.md phase 5).
!insertmacro MUI_LANGUAGE "English"

Section "${APP_NAME} (required)" SEC_APP
  SectionIn RO

  SetOutPath "$INSTDIR"
  ; A running Wraex keeps its own executable open, which would make the `File` below
  ; fail with a write error. Retry until it is gone; cancelling here changes nothing,
  ; because the executable is the first file written.
  retry_install_exe:
    ClearErrors
    Delete "$INSTDIR\${APP_EXE}"
    ${IfNot} ${Errors}
      Goto install_exe_ready
    ${EndIf}
    ; `/S` is the self-updater's path; a MessageBox there would wait forever, so fail
    ; with exit code 2 and let it quit Wraex first and retry.
    ${If} ${Silent}
      SetErrorLevel 2
      Abort "${APP_EXE} is still running; close ${APP_NAME} and retry."
    ${EndIf}
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION|MB_DEFBUTTON2 \
      "${APP_NAME} is still running. Quit it from its tray icon and choose Retry." \
      IDRETRY retry_install_exe
    SetErrorLevel 1
    Abort "The installation was cancelled: ${APP_EXE} is still running."
  install_exe_ready:

  File "${STAGE_DIR}\${APP_EXE}"
  ; A shortcut target and the Add/Remove Programs icon; the executable itself has no
  ; embedded icon yet (that would be a Windows resource in rust/crates/app's build).
  File "/oname=${APP_ICON}" "${ICON_FILE}"
  ; The packaged release as built: assets/ (game data, images, tray icon) next to the
  ; executable, plus the licence notices.
  File /r "${STAGE_DIR}\assets"
  File /r "${STAGE_DIR}\licences"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_ICON}" 0
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\${APP_ICON}" 0

  ; Add/Remove Programs (Settings > Apps) reads HKCU of the installing user. The
  ; InstallLocation value is what a silent upgrade's InstallDirRegKey reads back.
  WriteRegStr HKCU "${REG_UNINSTALL}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${REG_UNINSTALL}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${REG_UNINSTALL}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKCU "${REG_UNINSTALL}" "DisplayIcon" "$INSTDIR\${APP_ICON}"
  WriteRegStr HKCU "${REG_UNINSTALL}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${REG_UNINSTALL}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${REG_UNINSTALL}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${REG_UNINSTALL}" "NoModify" 1
  WriteRegDWORD HKCU "${REG_UNINSTALL}" "NoRepair" 1
  ; Apps & Features shows this size in KiB; IntFmt keeps it from going negative.
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${REG_UNINSTALL}" "EstimatedSize" "$0"
SectionEnd

Section "Desktop shortcut" SEC_DESKTOP
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_ICON}" 0
SectionEnd

; Not shown as a component: records the choice above for the next silent upgrade, and
; removes a shortcut the user just unticked.
Section "-Desktop shortcut choice"
  SectionGetFlags ${SEC_DESKTOP} $0
  IntOp $0 $0 & ${SF_SELECTED}
  ${If} $0 == ${SF_SELECTED}
    WriteRegDWORD HKCU "${REG_SETTINGS}" "DesktopShortcut" 1
  ${Else}
    Delete "$DESKTOP\${APP_NAME}.lnk"
    WriteRegDWORD HKCU "${REG_SETTINGS}" "DesktopShortcut" 0
  ${EndIf}
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_APP} "The application, the game data, images and licences it reads, and the uninstaller (required)."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_DESKTOP} "A ${APP_NAME} shortcut on the desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  ; The program files go first: a running Wraex locks wraex.exe, and cancelling here
  ; leaves the installation untouched rather than half removed.
  retry_uninstall_exe:
    ClearErrors
    Delete "$INSTDIR\${APP_EXE}"
    ${IfNot} ${Errors}
      Goto uninstall_exe_ready
    ${EndIf}
    ; `QuietUninstallString` (/S): no MessageBox, so an unattended run cannot hang.
    ${If} ${Silent}
      SetErrorLevel 2
      Abort "${APP_EXE} is still running; close ${APP_NAME} and retry."
    ${EndIf}
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION|MB_DEFBUTTON2 \
      "${APP_NAME} is still running. Quit it from its tray icon and choose Retry." \
      IDRETRY retry_uninstall_exe
    MessageBox MB_OK|MB_ICONINFORMATION "Nothing was removed. Run the uninstaller again once ${APP_NAME} has quit."
    SetErrorLevel 1
    Quit
  uninstall_exe_ready:

  Delete "$INSTDIR\${APP_ICON}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR\assets"
  RMDir /r "$INSTDIR\licences"
  ; Deliberately not $APPDATA: settings, the log and the config imported from
  ; Awakened PoE Trade stay, so uninstalling and reinstalling loses nothing.
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegKey HKCU "${REG_UNINSTALL}"
  DeleteRegKey HKCU "${REG_SETTINGS}"
SectionEnd

; Below the sections because `${SEC_DESKTOP}` only exists once they are declared.

Function .onInit
  ; $SMPROGRAMS, $DESKTOP and $APPDATA are the current user's; the explicit context
  ; keeps a later `SetShellVarContext all` in some included macro from leaking in.
  SetShellVarContext current

  ; `InstallDirRegKey` hands NSIS the stored path with a trailing backslash. Drop it
  ; (unless it is a drive root) so the `"$INSTDIR\file"` paths below do not end up as
  ; `...\Wraex\\wraex.exe` in shortcuts and the registry on an upgrade.
  StrCpy $1 $INSTDIR 1 -1
  ${If} $1 == "\"
    StrCpy $1 $INSTDIR -1
    ${If} $1 != ":"
      StrCpy $INSTDIR $1
    ${EndIf}
  ${EndIf}

  ; Honour the desktop-shortcut choice from a previous install, because a silent
  ; upgrade never shows the components page. An absent value means a first install,
  ; which leaves the section's default (ticked); 0 means the user unticked it.
  ReadRegDWORD $0 HKCU "${REG_SETTINGS}" "DesktopShortcut"
  ${If} $0 == 0
    SectionSetFlags ${SEC_DESKTOP} 0
  ${EndIf}
FunctionEnd

Function un.onInit
  SetShellVarContext current
FunctionEnd

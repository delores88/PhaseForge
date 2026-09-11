# Use NSIS's own File instruction and compression for the application payload.
# The default external Nsis7z and NSISunz plugins bundle obsolete decoders,
# including in the NSIS 3.12 toolset. Keep electron-builder's standard wizard,
# architecture checks, shortcuts, uninstall and process-close handling.
# This include is deliberately restricted to our separate native packages.

# Present the generated, pinned license page exactly once, including updates.
# The stock assisted-template license location skips --updated installations.
!ifndef BUILD_UNINSTALLER
  !ifmacrodef licensePage
    !macroundef licensePage
  !else
    !error "The configured Microsoft runtime agreement is required."
  !endif
!endif
!macro customWelcomePage
  !define MUI_LICENSEPAGE_CHECKBOX
  !define MUI_LICENSEPAGE_CHECKBOX_TEXT "I accept the Microsoft runtime terms."
  !insertmacro MUI_PAGE_LICENSE "${PROJECT_DIR}\..\tools\third-party\msvc-runtime\END-USER-TERMS.txt"
!macroend

# Silent deployments cannot imply assent merely by hiding the wizard.
!macro customInit
  ${If} ${Silent}
    ${GetParameters} $R0
    ClearErrors
    ${GetOptions} $R0 "/ACCEPT_MSVC_TERMS=" $R1
    ${If} ${Errors}
      SetErrorLevel 2
      Quit
    ${EndIf}
    ${If} $R1 != "MSVC-2026-09-11"
      SetErrorLevel 2
      Quit
    ${EndIf}
  ${EndIf}
!macroend

!ifdef APP_PACKAGE_URL
  !error "PhaseForge requires a complete offline installer, not nsis-web."
!endif
!ifdef APP_32
  !error "PhaseForge Windows packages support x64 and ARM64 only."
!endif
!ifdef APP_64
  !ifdef APP_ARM64
    !error "Build Windows x64 and ARM64 installers separately."
  !endif
  !define APP_BUILD_DIR "${PROJECT_DIR}\dist\win-unpacked"
!else
  !ifdef APP_ARM64
    !define APP_BUILD_DIR "${PROJECT_DIR}\dist\win-arm64-unpacked"
  !else
    !error "Missing native Windows payload architecture."
  !endif
!endif
!if /FileExists "${APP_BUILD_DIR}\${PRODUCT_FILENAME}.exe"
!else
  !error "Native Windows payload was not staged in the configured desktop/dist directory."
!endif

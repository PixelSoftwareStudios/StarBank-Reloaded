!define STARBANK_BANK_PROGID "StarBankReloaded.SC2Bank"
!define STARBANK_BANK_CLASS "Software\Classes\${STARBANK_BANK_PROGID}"
!define STARBANK_BANK_OPEN_WITH "Software\Classes\.sc2bank\OpenWithProgids"

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "${STARBANK_BANK_CLASS}" "" "StarCraft II Bank"
  WriteRegStr HKCU "${STARBANK_BANK_CLASS}\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr HKCU "${STARBANK_BANK_CLASS}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  WriteRegStr HKCU "${STARBANK_BANK_OPEN_WITH}" "${STARBANK_BANK_PROGID}" ""
  !insertmacro UPDATEFILEASSOC
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Push $R0
  ReadRegStr $R0 HKCU "${STARBANK_BANK_CLASS}\shell\open\command" ""
  ${If} $R0 == '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
    DeleteRegValue HKCU "${STARBANK_BANK_OPEN_WITH}" "${STARBANK_BANK_PROGID}"
    DeleteRegKey HKCU "${STARBANK_BANK_CLASS}"
    !insertmacro UPDATEFILEASSOC
  ${EndIf}
  Pop $R0
!macroend

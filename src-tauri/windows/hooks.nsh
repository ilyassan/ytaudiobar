; Tauri's default NSIS template only writes the extension -> ProgID mapping
; for fileAssociations (Software\Classes\.mp3 -> "Audio File" -> shell\open\command).
; It never registers the app under Software\Classes\Applications\<exe>\SupportedTypes,
; which is what Windows Explorer's "Open with" picker and Settings > Default apps
; actually read to know YTAudioBar is a selectable app at all -- so without this,
; the app never appears in "Open with" even though the ProgID association is correct.
; See https://github.com/tauri-apps/tauri/issues/9803.
;
; SHCTX resolves to HKCU/HKLM based on this app's installMode (currentUser here),
; so this stays correct if that ever changes.

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "FriendlyAppName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'

  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mp3" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".m4a" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".mp4" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".flac" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".ogg" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".opus" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".wav" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".aac" ""
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".webm" ""

  ; Tell Explorer the Open With list changed instead of waiting for its own cache to expire.
  System::Call "shell32::SHChangeNotify(i,i,i,i)(0x08000000,0x1000,0,0)"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe"
  System::Call "shell32::SHChangeNotify(i,i,i,i)(0x08000000,0x1000,0,0)"
!macroend

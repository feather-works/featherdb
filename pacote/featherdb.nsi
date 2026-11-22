; Instalador do FeatherDB para Windows (NSIS). Gerado por pacote/gerar.sh:
;   makensis -DVERSAO=0.1.0 -DEXE=caminho/featherdb.exe -DSAIDA=caminho/instalador.exe pacote/featherdb.nsi
; Instala para todos os usuários em "Arquivos de Programas\FeatherWorks\FeatherDB" (pede
; administrador uma vez: só o administrador altera os .exe; a pasta FeatherWorks é da suíte), cria atalhos e registra o desinstalador em "Aplicativos instalados".

Unicode true
!include "MUI2.nsh"

!ifndef VERSAO
  !define VERSAO "0.0.0"
!endif

Name "FeatherDB ${VERSAO}"
OutFile "${SAIDA}"
InstallDir "$PROGRAMFILES64\FeatherWorks\FeatherDB"
InstallDirRegKey HKLM "Software\FeatherWorks\FeatherDB" "Pasta"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define MUI_ICON "icones\featherdb.ico"
!define MUI_UNICON "icones\featherdb.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\featherdb.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Abrir o FeatherDB"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "PortugueseBR"

!include "FileFunc.nsh"
!define DESINSTALAR "Software\Microsoft\Windows\CurrentVersion\Uninstall\FeatherDB"

; Atalhos de todos os usuários e o registro de 64 bits (o programa é de 64 bits).
Function .onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Function un.onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Section "FeatherDB"
  SetOutPath "$INSTDIR"
  ; Com o programa aberto, o .exe em uso não se sobrescreve, mas se renomeia; o .old sai
  ; na próxima instalação ou na desinstalação.
  Delete "$INSTDIR\featherdb.old"
  Rename "$INSTDIR\featherdb.exe" "$INSTDIR\featherdb.old"
  File "/oname=featherdb.exe" "${EXE}"
  File "icones\featherdb.ico"
  File "/oname=LICENSE.txt" "..\LICENSE"
  WriteUninstaller "$INSTDIR\desinstalar.exe"

  CreateShortCut "$SMPROGRAMS\FeatherDB.lnk" "$INSTDIR\featherdb.exe" "" "$INSTDIR\featherdb.ico"
  CreateShortCut "$DESKTOP\FeatherDB.lnk" "$INSTDIR\featherdb.exe" "" "$INSTDIR\featherdb.ico"

  WriteRegStr HKLM "Software\FeatherWorks\FeatherDB" "Pasta" "$INSTDIR"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayName" "FeatherDB"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayVersion" "${VERSAO}"
  WriteRegStr HKLM "${DESINSTALAR}" "Publisher" "FeatherWorks"
  ; Tamanho em "Programas e Recursos" (KB, do que foi instalado).
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${DESINSTALAR}" "EstimatedSize" "$0"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayIcon" "$INSTDIR\featherdb.ico"
  WriteRegStr HKLM "${DESINSTALAR}" "URLInfoAbout" "https://github.com/feather-works/featherdb"
  WriteRegStr HKLM "${DESINSTALAR}" "UninstallString" '"$INSTDIR\desinstalar.exe"'
  WriteRegStr HKLM "${DESINSTALAR}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKLM "${DESINSTALAR}" "NoModify" 1
  WriteRegDWORD HKLM "${DESINSTALAR}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\FeatherDB.lnk"
  Delete "$DESKTOP\FeatherDB.lnk"
  Delete "$INSTDIR\featherdb.exe"
  Delete "$INSTDIR\featherdb.old"
  Delete "$INSTDIR\featherdb.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\desinstalar.exe"
  RMDir "$INSTDIR"
  ; A pasta da suíte só sai se ficou vazia (sem outro programa nem o Launcher).
  RMDir "$INSTDIR\.."
  DeleteRegKey HKLM "Software\FeatherWorks\FeatherDB"
  DeleteRegKey /ifempty HKLM "Software\FeatherWorks"
  DeleteRegKey HKLM "${DESINSTALAR}"
SectionEnd

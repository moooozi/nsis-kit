; Minimal consumer: no application integrations or custom build tool.
!define APP_NAME "Example App"
!define APP_EXE "example.exe"
!define APP_DIRECTORY "$PROGRAMFILES64\Example App"
!define APP_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ExampleApp"
!define APP_PUBLISHER "Example Publisher"
!define APP_DESCRIPTION "A minimal desktop application using the native NSIS toolkit."
!define APP_LOGO "${__FILEDIR__}\..\..\ui\examples\logo.svg"
!include "${__FILEDIR__}\..\..\setup.nsh"

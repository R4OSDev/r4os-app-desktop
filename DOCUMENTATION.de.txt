R4DESK.R4X
===========

R4DESK ist die externe Desktop-Anwendung und der Desktop-Host von R4OS.

Zustaendigkeit
--------------

- Desktopflaeche, Taskbar, Startmenue und Systemdialoge
- gehostete GUI-Fenster
- Terminalfenster und Terminalmodus als Console-Hosts
- Tastatur-/Mausdispatch, Fokus und Fensterlebenszyklus
- Clipboard-Integration
- Desktop- und App-Present
- Starten, Klassifizieren, Schliessen und Anzeigen von Programminstanzen

API-Nutzung
-----------

Desktop validiert R4XStart, erzeugt das Gruppen-Bundle und nutzt R4SYS,
R4DESK, R4DRAW, R4NET, R4AUDIO und R4DEV gemaess module.R4MF. Zusaetzlich
bindet er die R4STD-Tabellen SETTINGS_V1, TIME_V1 und CONFIG_V1 lokal ein.
Optionale Kernel-Felder werden mit hasFn geprueft.

Fenster-, Console-, Clipboard-, ProgramStatus- und Power-Funktionen laufen
ueber ihre aktuellen Gruppenfelder. Der Desktop besitzt keine eigene
Kernel- oder API-Parallelstruktur.

Konfiguration
-------------

Produktive Konfiguration liegt unter C:/R4OS/CONFIG. Desktop-Layouts,
Settings und R4S-Dateien werden ueber die geladene R4STD-Runtime verarbeitet;
CONFIG_V1 nutzt dabei den caller-eigenen R4SYS-Kontext.

Start -> Settings -> Appearance startet APPEARANCE.R4X. Die Anwendung
aktualisiert DESKTOP_BG atomar in DESKTOP.R4S und signalisiert dem Desktop
die gespeicherte Farbe ueber den vorhandenen GUI-Revisionskanal. Der Desktop
verifiziert den Wert gegen die Datei und uebernimmt ihn ohne Neustart oder
periodisches Dateipolling.

Build und Test
--------------

    DevTools/Scripts/Build.bat -app Desktop
    DevTools/Scripts/Build.bat -app Appearance
    DevTools/Scripts/Build.bat -norun

Interaktive Sichttests erfolgen ueber Build.bat -gui. Headless API-/Bootsmokes
laufen ueber Build.bat -test.

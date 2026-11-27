### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossar für die Prüfung
### Wochentage und Monate stehen in normaler Groß- und Kleinschreibung (Montag, Januar).
### Kurze Monatsnamen folgen der üblichen deutschen Abkürzung ohne Punkt (Jan, Feb, Mär).

## Kalendernamen für Datumsangaben. Die Nummern folgen ISO: Montag und Januar sind 1.

date-weekday =
    { $day ->
        [1] Montag
        [2] Dienstag
        [3] Mittwoch
        [4] Donnerstag
        [5] Freitag
        [6] Samstag
       *[7] Sonntag
    }

date-month =
    { $month ->
        [1] Januar
        [2] Februar
        [3] März
        [4] April
        [5] Mai
        [6] Juni
        [7] Juli
        [8] August
        [9] September
        [10] Oktober
        [11] November
       *[12] Dezember
    }

date-month-short =
    { $month ->
        [1] Jan
        [2] Feb
        [3] Mär
        [4] Apr
        [5] Mai
        [6] Jun
        [7] Jul
        [8] Aug
        [9] Sep
        [10] Okt
        [11] Nov
       *[12] Dez
    }

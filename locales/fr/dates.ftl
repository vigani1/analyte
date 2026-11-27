### Review status: machine-translated, needs native review (2026-11-27)
###
### Glossaire pour la relecture
### sample = échantillon ; sample ID = ID d’échantillon ; result = résultat ; test = analyse
### rerun = réanalyse ; resend = renvoi ; reprocess = retraitement
### held for review = retenu pour vérification ; Needs review = À vérifier
### machine = automate ; analyzer = analyseur ; lab app = application de laboratoire
### delivery = transmission ; order = demande ; record only = enregistrement seul

## Calendar names, for dates people read. Numbers are ISO: Monday and January are 1.

date-weekday =
    { $day ->
        [1] lundi
        [2] mardi
        [3] mercredi
        [4] jeudi
        [5] vendredi
        [6] samedi
       *[7] dimanche
    }

date-month =
    { $month ->
        [1] janvier
        [2] février
        [3] mars
        [4] avril
        [5] mai
        [6] juin
        [7] juillet
        [8] août
        [9] septembre
        [10] octobre
        [11] novembre
       *[12] décembre
    }

date-month-short =
    { $month ->
        [1] janv.
        [2] févr.
        [3] mars
        [4] avr.
        [5] mai
        [6] juin
        [7] juil.
        [8] août
        [9] sept.
        [10] oct.
        [11] nov.
       *[12] déc.
    }

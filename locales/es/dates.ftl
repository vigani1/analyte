### Review status: machine-translated, needs native review (2026-11-27)
###
### Glosario para la revisión
### Los nombres de días y meses van en minúscula, como es habitual en español.

## Nombres del calendario. Los números son ISO: el lunes y enero son 1.

date-weekday =
    { $day ->
        [1] lunes
        [2] martes
        [3] miércoles
        [4] jueves
        [5] viernes
        [6] sábado
       *[7] domingo
    }

date-month =
    { $month ->
        [1] enero
        [2] febrero
        [3] marzo
        [4] abril
        [5] mayo
        [6] junio
        [7] julio
        [8] agosto
        [9] septiembre
        [10] octubre
        [11] noviembre
       *[12] diciembre
    }

date-month-short =
    { $month ->
        [1] ene
        [2] feb
        [3] mar
        [4] abr
        [5] may
        [6] jun
        [7] jul
        [8] ago
        [9] sept
        [10] oct
        [11] nov
       *[12] dic
    }

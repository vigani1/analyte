### English source wording.

## Calendar names, for dates people read. Numbers are ISO: Monday and January are 1.

date-weekday =
    { $day ->
        [1] Monday
        [2] Tuesday
        [3] Wednesday
        [4] Thursday
        [5] Friday
        [6] Saturday
       *[7] Sunday
    }

date-month =
    { $month ->
        [1] January
        [2] February
        [3] March
        [4] April
        [5] May
        [6] June
        [7] July
        [8] August
        [9] September
        [10] October
        [11] November
       *[12] December
    }

date-month-short =
    { $month ->
        [1] Jan
        [2] Feb
        [3] Mar
        [4] Apr
        [5] May
        [6] Jun
        [7] Jul
        [8] Aug
        [9] Sep
        [10] Oct
        [11] Nov
       *[12] Dec
    }

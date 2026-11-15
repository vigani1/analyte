# Autolyser versions

Only the internal host format described in the Autolyser / Autolyser ISE
Operator's Manual **Rev18**, chapter 23, is implemented. This is a manual
revision, not a claim to support every installed software version.

| Variant | Difference that matters | Source |
| --- | --- | --- |
| Controls disabled | Basic patient/list/sample/result layout | Rev18 section 23.2, p.177 |
| Controls enabled | Adds control kind, level and lot fields | Rev18 section 23.2, p.178 |
| Internal checksum | Byte sum modulo 256, three decimal digits | Rev18 section 23.3, p.179 |

The controls variant is configured explicitly, never detected by guessing.
Other revisions, variable-script protocols and undocumented TCP behavior are
not qualified. There is no edition negotiation; another installed format
requires evidence and a reviewed configuration or implementation change.

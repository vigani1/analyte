# VIDAS versions

Only the VIDAS PC Literal family, normal and Alternate ACK modes, is implemented.
The research identifies BCI Link 161150-486 revisions A/B and VIDAS PC User
Manual F; these are documented implementation inputs, not verified installed
software compatibility. Original manual section verification: not found.

| Setting | Difference that matters | Source |
| --- | --- | --- |
| Literal / Alternate | Packet ACK before / after ETX | Source: not found; research section 3 and current framing/dialog code |
| Twelve / Seventeen | Explicit installed sample-ID ceiling, never truncation | Source: not found; research section 4 and `SampleLimit` |
| Checksum and suffix policy | Explicit configured span, case and suffix treatment | Source: not found; current framing configuration |

Mini VIDAS, VIDAS 3, XML/FTP and unknown PC profiles are not supported by
selecting Literal. Another revision requires installed evidence and reviewed
settings; there is no software-version negotiation or guessed fallback.

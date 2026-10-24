# Gearwise driver adaptation for Open HUB

Source: https://github.com/Dhiva-Labs/gearwise , downloaded 2026-10-22.
MIT, Copyright (c) 2026 Dhivakar Rajaram (Dhiva Labs); LICENSE retained.
Only core, HID, Razer, Corsair, SteelSeries and input crates are included.
No Gearwise Logitech driver is included: existing Open HUB Logitech and MCHOSE
paths remain authoritative. Driver capabilities must be checked in code;
the upstream README describes app features, not every brand's capabilities.
Razer and Corsair upstream drivers explicitly reject button assignments.
SteelSeries rejects onboard macros. These gaps must not be hidden by demo data.

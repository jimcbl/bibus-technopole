# Bibus Technopôle — Rust

Click the bus icon in Ubuntu's top bar or the macOS menu bar to see **three departures total**:
the nearest line 2 bus and the following two, from **Technopôle** toward
**P de Commerce via Liberté**. Times start from the moment you open the menu.
There is no longer a 16:30 filter.

The Linux executable is approximately **2 MB**, compared with approximately
69 MB for the earlier Python/Qt version. The downloaded timetable cache uses
additional disk space (about 2 MB with the current feed).

Opening the menu requests fresh Bibus data. The last usable result appears
immediately while that request completes, and then updates. The app also
refreshes every minute. Each departure shows its departure time, minutes to
departure, and whether it is live or scheduled. Known cancellations and skipped
stops are excluded from the next three buses. If fewer than three remain today,
the menu shows the available buses; after the last one it says none remain.

The dropdown uses one heading and no separator rows, and uses tighter spacing
between each departure’s time and status. Secondary actions are under **More**.
Native row height and padding remain controlled by Ubuntu’s theme (or macOS).

English is the default. Choose **More → Français** or **Plus → English** to
switch languages; the choice is saved for future launches.

## Run on Ubuntu

Quit the previous version first, then open **Bibus-Technopole**. This is a
standalone, statically linked Rust executable for Intel/AMD 64-bit Linux.
No Python, Qt, GTK development packages, or Rust installation is needed.

The application uses the system's `curl` command for HTTPS downloads, with
certificate verification enabled. If it is missing, install it with
`sudo apt install curl`. Ubuntu's **Ubuntu AppIndicators** extension must be
enabled for the top-bar icon. The executable does not install itself or enable
startup at login.

If executable permission was lost during download, enable **Executable as
Program** in the file's properties. To test the feed from a terminal without
registering an icon, run `./Bibus-Technopole --check`.

## macOS

The Apple Silicon build is available as `dist/Bibus-Technopole-mac-arm64.zip`.
Unzip it, then open **Bibus Technopole.app**. You can move the app to
**Applications** first. Quit any previous version before opening the new build.
The app shows a white bus icon in the menu bar and has no Dock icon. Choose
**More → Quit** to exit.

The bundle is locally signed, but is not notarized by Apple. It requires an
Apple Silicon Mac running macOS 11 or later. It does not enable startup at login.

The native macOS backend uses AppKit through `tray-icon` and `tao`. To build
from source **on a Mac**, install a current stable Rust toolchain and the Xcode
Command Line Tools, and accept the Xcode licence if prompted:

```sh
cargo build --release --locked
./target/release/bibus-technopole
```

The release build and all ten unit tests pass natively on Apple Silicon.
The menu-bar UI has not been automatically tested. To check the feeds without
opening the UI, run `./target/release/bibus-technopole --check`.

## Build from source

With a Rust toolchain and a C linker available:

```sh
cargo build --release --locked
cargo test --locked
```

For a statically linked Linux executable, install the Rust musl target and
appropriate linker, then use `--target x86_64-unknown-linux-musl`. The shipped
build uses size optimisation, link-time optimisation, and stripped symbols.
Linux uses the freedesktop StatusNotifierItem protocol through `ksni` rather
than bundling a GUI framework.

Validation covers ten unit tests for calendars, delays, cancellations, stale
predictions, and the three-departure limit. The published Bibus feeds were also
checked directly, and the Ubuntu tray menu was queried through D-Bus to verify
registration, menu opening, departure count, English/French switching, language
persistence, and quitting.

## Data

Source: Bibus / RD Brest / Brest Métropole, through the
[published dataset](https://transport.data.gouv.fr/datasets/horaires-theoriques-et-temps-reel-des-bus-et-tramways-circulant-sur-le-territoire-de-brest-metropole).
The data remains subject to its published licence and terms.

The app combines GTFS calendars and holiday exceptions with explicit GTFS-RT
predictions for stop `TECHNO_1`, line 2, direction 1. These identifiers were
checked against the November 2026 feed. Network changes may require updating
them. All times use Europe/Paris, independently of your computer's timezone.

Timetables are cached in `~/.cache/bibus-technopole-rust` on Linux (or under
`XDG_CACHE_HOME`), and in `~/Library/Caches/bibus-technopole-rust` on macOS.
The app checks for timetable updates daily, rejects schedules outside their
validity period, and rejects live feeds over five minutes old. A valid cached
timetable supports scheduled departures when offline. It does not cache live
predictions on disk.

The app contacts only the two transport endpoints linked from the dataset.
No account or API key is required. The Source menu item opens that dataset
in your browser. There is no journey planning or destination arrival estimate.

use crate::realtime;
use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Europe::Paris;
use prost::Message;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read},
    path::PathBuf,
    process::Command,
    time::SystemTime,
};

pub const SOURCE: &str = "https://transport.data.gouv.fr/datasets/horaires-theoriques-et-temps-reel-des-bus-et-tramways-circulant-sur-le-territoire-de-brest-metropole";
const STATIC: &str = "https://s3.eu-west-1.amazonaws.com/files.orchestra.ratpdev.com/networks/bibus/exports/medias.zip";
const LIVE: &str = "https://proxy.transport.data.gouv.fr/resource/bibus-brest-gtfs-rt-trip-update";
const STOP: &str = "TECHNO_1";

fn path(cache: bool) -> PathBuf {
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(if cache {
            "Library/Caches"
        } else {
            "Library/Application Support"
        })
    } else {
        std::env::var_os(if cache {
            "XDG_CACHE_HOME"
        } else {
            "XDG_CONFIG_HOME"
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(if cache {
                ".cache"
            } else {
                ".config"
            })
        })
    };
    base.join("bibus-technopole-rust")
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    // Use the OS HTTPS client/certificate store rather than bundling TLS libraries.
    // No shell and no user-controlled arguments; -q ignores local curl configuration.
    let result = Command::new("curl")
        .args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "8",
            "--max-time",
            "20",
            "--max-filesize",
            "25000000",
            "--user-agent",
            "Bibus-Technopole-Rust/0.2",
            url,
        ])
        .output()
        .map_err(|_| "Le client HTTPS « curl » est introuvable.".to_string())?;
    if !result.status.success() {
        return Err("Connexion Bibus indisponible.".into());
    }
    Ok(result.stdout)
}

#[derive(Deserialize)]
struct Route {
    route_id: String,
    route_short_name: String,
}
#[derive(Deserialize)]
struct Trip {
    trip_id: String,
    route_id: String,
    service_id: String,
    direction_id: String,
}
#[derive(Deserialize)]
struct StopTime {
    trip_id: String,
    stop_id: String,
    stop_sequence: u32,
    departure_time: String,
    #[serde(default)]
    pickup_type: String,
}
#[derive(Deserialize)]
struct Calendar {
    service_id: String,
    start_date: String,
    end_date: String,
    monday: String,
    tuesday: String,
    wednesday: String,
    thursday: String,
    friday: String,
    saturday: String,
    sunday: String,
}
#[derive(Deserialize)]
struct Exception {
    service_id: String,
    date: String,
    exception_type: String,
}
#[derive(Deserialize)]
struct Info {
    #[serde(default)]
    feed_start_date: String,
    #[serde(default)]
    feed_end_date: String,
}

struct Timetable {
    trips: HashMap<String, Trip>,
    stops: Vec<StopTime>,
    calendar: Vec<Calendar>,
    exceptions: Vec<Exception>,
    first: String,
    last: String,
}
fn rows<T: serde::de::DeserializeOwned>(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<T>, String> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound)
            if name == "calendar.txt"
                || name == "calendar_dates.txt"
                || name == "feed_info.txt" =>
        {
            return Ok(vec![])
        }
        Err(_) => return Err(format!("Fichier d’horaires manquant : {name}")),
    };
    // Bound decompression; only named CSV files are read, nothing is extracted.
    csv::Reader::from_reader(file.take(80_000_000))
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("Horaires illisibles : {name}"))
}
fn key(day: NaiveDate) -> String {
    day.format("%Y%m%d").to_string()
}
fn seconds(value: &str) -> Option<i64> {
    let parts: Vec<i64> = value
        .split(':')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.len() != 3
        || parts[0] < 0
        || !(0..60).contains(&parts[1])
        || !(0..60).contains(&parts[2])
    {
        return None;
    }
    Some(parts[0] * 3600 + parts[1] * 60 + parts[2])
}
impl Timetable {
    fn parse(data: &[u8]) -> Result<Self, String> {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(data)).map_err(|_| "Archive Bibus illisible.")?;
        let routes: HashSet<_> = rows::<Route>(&mut archive, "routes.txt")?
            .into_iter()
            .filter(|r| r.route_short_name == "2")
            .map(|r| r.route_id)
            .collect();
        let trips: HashMap<_, _> = rows::<Trip>(&mut archive, "trips.txt")?
            .into_iter()
            .filter(|t| routes.contains(&t.route_id) && t.direction_id == "1")
            .map(|t| (t.trip_id.clone(), t))
            .collect();
        let stops: Vec<_> = rows::<StopTime>(&mut archive, "stop_times.txt")?
            .into_iter()
            .filter(|s| {
                s.stop_id == STOP
                    && trips.contains_key(&s.trip_id)
                    && s.pickup_type != "1"
                    && seconds(&s.departure_time).is_some()
            })
            .collect();
        if stops.is_empty() {
            return Err("La ligne 2 à Technopôle a changé. Vérifiez les horaires.".into());
        }
        let info = rows::<Info>(&mut archive, "feed_info.txt")?
            .into_iter()
            .next();
        Ok(Self {
            trips,
            stops,
            calendar: rows(&mut archive, "calendar.txt")?,
            exceptions: rows(&mut archive, "calendar_dates.txt")?,
            first: info
                .as_ref()
                .map(|i| i.feed_start_date.clone())
                .unwrap_or_default(),
            last: info.map(|i| i.feed_end_date).unwrap_or_default(),
        })
    }
    fn active(&self, day: NaiveDate) -> HashSet<String> {
        let date = key(day);
        let mut active: HashSet<_> = self
            .calendar
            .iter()
            .filter(|c| {
                let days = [
                    &c.monday,
                    &c.tuesday,
                    &c.wednesday,
                    &c.thursday,
                    &c.friday,
                    &c.saturday,
                    &c.sunday,
                ];
                c.start_date <= date
                    && date <= c.end_date
                    && days[day.weekday().num_days_from_monday() as usize] == "1"
            })
            .map(|c| c.service_id.clone())
            .collect();
        for exception in self.exceptions.iter().filter(|e| e.date == date) {
            if exception.exception_type == "1" {
                active.insert(exception.service_id.clone());
            } else {
                active.remove(&exception.service_id);
            }
        }
        active
    }
    fn departures(&self, now: i64) -> Vec<Departure> {
        let today = Paris.timestamp_opt(now, 0).unwrap().date_naive();
        let mut result = vec![];
        for day in [today - Duration::days(1), today] {
            let active = self.active(day);
            let origin = Paris
                .from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
                .unwrap()
                .timestamp()
                - 43200;
            for stop in &self.stops {
                if !active.contains(&self.trips[&stop.trip_id].service_id) {
                    continue;
                }
                let timestamp = origin + seconds(&stop.departure_time).unwrap();
                if Paris.timestamp_opt(timestamp, 0).unwrap().date_naive() != today {
                    continue;
                }
                result.push(Departure {
                    trip: stop.trip_id.clone(),
                    date: key(day),
                    sequence: stop.stop_sequence,
                    scheduled: timestamp,
                    expected: timestamp,
                    live: false,
                    cancelled: false,
                });
            }
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct Departure {
    trip: String,
    date: String,
    sequence: u32,
    scheduled: i64,
    expected: i64,
    live: bool,
    cancelled: bool,
}
fn fresh(timestamp: i64, now: i64) -> bool {
    (-60..=300).contains(&(now - timestamp))
}
fn apply_live(departures: &mut [Departure], bytes: &[u8], now: i64) -> Result<i64, String> {
    let feed = realtime::Feed::decode(bytes).map_err(|_| "Flux temps réel illisible.")?;
    let timestamp = feed.header.timestamp.ok_or("Flux sans date.")? as i64;
    if !fresh(timestamp, now) || feed.header.incrementality.unwrap_or(0) != 0 {
        return Err("Flux temps réel trop ancien ou partiel.".into());
    }
    let mut updates: HashMap<&str, Vec<&realtime::Update>> = HashMap::new();
    for entity in &feed.entity {
        if entity.deleted.unwrap_or(false) {
            continue;
        }
        if let Some(update) = &entity.update {
            if let Some(id) = update.trip.id.as_deref() {
                updates.entry(id).or_default().push(update);
            }
        }
    }
    for departure in departures {
        for update in updates.get(departure.trip.as_str()).into_iter().flatten() {
            if update
                .trip
                .date
                .as_ref()
                .is_some_and(|date| !date.is_empty() && date != &departure.date)
                || update.timestamp.is_some_and(|t| !fresh(t as i64, now))
            {
                continue;
            }
            if update.trip.relationship == Some(3) {
                departure.cancelled = true;
                departure.live = true;
                break;
            }
            for stop in &update.stops {
                let matches = stop
                    .sequence
                    .map(|s| s == departure.sequence)
                    .unwrap_or(stop.id.as_deref() == Some(STOP));
                if !matches
                    || stop
                        .id
                        .as_deref()
                        .is_some_and(|id| !id.is_empty() && id != STOP)
                {
                    continue;
                }
                if stop.relationship == Some(1) {
                    departure.cancelled = true;
                    departure.live = true;
                    break;
                }
                if stop.relationship == Some(2) {
                    break;
                }
                if let Some(event) = &stop.departure {
                    if let Some(expected) = event
                        .time
                        .or_else(|| event.delay.map(|d| departure.scheduled + i64::from(d)))
                    {
                        if (expected - departure.scheduled).abs() < 21600 {
                            departure.expected = expected;
                            departure.live = true;
                        }
                    }
                }
                break;
            }
            break;
        }
    }
    Ok(timestamp)
}

pub struct Snapshot {
    departures: Vec<Departure>,
    fetched: i64,
    live_at: Option<i64>,
    notices: Vec<String>,
}
#[derive(Default)]
pub struct Provider {
    table: Option<Timetable>,
    next_check: i64,
}
impl Provider {
    pub fn fetch(&mut self) -> Result<Snapshot, String> {
        let now = Utc::now().timestamp();
        let mut notices = vec![];
        if self.table.is_none() || now >= self.next_check {
            let cache = path(true);
            fs::create_dir_all(&cache).map_err(|_| "Impossible de créer le cache des horaires.")?;
            let file = cache.join("timetable.zip");
            let recent = file
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age.as_secs() < 86400);
            let load = if recent {
                fs::read(&file).map_err(|_| "Cache illisible.".into())
            } else {
                download(STATIC)
            };
            match load.and_then(|bytes| {
                let table = Timetable::parse(&bytes)?;
                if !recent {
                    let temp = file.with_extension("tmp");
                    fs::write(&temp, &bytes)
                        .and_then(|_| fs::rename(temp, &file))
                        .map_err(|_| "Écriture du cache impossible.")?;
                }
                Ok(table)
            }) {
                Ok(table) => {
                    self.table = Some(table);
                    self.next_check = now + 3600;
                }
                Err(_) => {
                    if self.table.is_none() {
                        self.table = fs::read(&file).ok().and_then(|b| Timetable::parse(&b).ok());
                    }
                    if self.table.is_none() {
                        return Err(
                            "Horaires indisponibles. Vérifiez la connexion et la présence de curl."
                                .into(),
                        );
                    }
                    self.next_check = now + 300;
                    notices.push("Mise à jour indisponible · horaires en cache".into());
                }
            }
        }
        let table = self.table.as_ref().unwrap();
        let today = key(Paris.timestamp_opt(now, 0).unwrap().date_naive());
        if (!table.first.is_empty() && today < table.first)
            || (!table.last.is_empty() && today > table.last)
        {
            self.next_check = 0;
            return Err("Horaires expirés. Réessayez en ligne.".into());
        }
        let mut departures = table.departures(now);
        let live_at = download(LIVE)
            .and_then(|bytes| apply_live(&mut departures, &bytes, now))
            .ok();
        if live_at.is_none() {
            notices.push("Temps réel indisponible · horaires prévus".into());
        }
        Ok(Snapshot {
            departures,
            fetched: Utc::now().timestamp(),
            live_at,
            notices,
        })
    }
}

pub struct Model {
    pub snapshot: Option<Snapshot>,
    pub english: bool,
    error: String,
}
impl Model {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            english: fs::read_to_string(path(false).join("language"))
                .map(|value| value.trim() != "fr")
                .unwrap_or(true),
            error: String::new(),
        }
    }
    pub fn set_english(&mut self, english: bool) {
        self.english = english;
        let directory = path(false);
        if fs::create_dir_all(&directory)
            .and_then(|_| {
                fs::write(
                    directory.join("language"),
                    if english { "en" } else { "fr" },
                )
            })
            .is_err()
        {
            eprintln!("Bibus: unable to save language preference.");
        }
    }
    pub fn text(&self, french: &str, english: &str) -> String {
        if self.english {
            english.into()
        } else {
            french.into()
        }
    }
    fn translate(&self, value: &str) -> String {
        if !self.english {
            return value.into();
        }
        let mut result = value.to_string();
        for (french, english) in [
            ("Chargement des départs…", "Loading departures…"),
            (
                "Aucun autre départ aujourd’hui après cet horaire.",
                "No more departures today.",
            ),
            ("Prochain : ", "Next: "),
            (
                "Temps réel indisponible · horaires prévus",
                "Live data unavailable · scheduled times",
            ),
            (
                "Mise à jour indisponible · horaires en cache",
                "Update unavailable · cached timetable",
            ),
            (
                "Horaires indisponibles. Vérifiez la connexion et la présence de curl.",
                "Timetable unavailable. Check your connection and curl installation.",
            ),
            (
                "Horaires expirés. Réessayez en ligne.",
                "Timetable expired. Reconnect and retry.",
            ),
            (
                "Impossible de créer le cache des horaires.",
                "Unable to create the timetable cache.",
            ),
            ("Actualisé à ", "Updated "),
            ("actualisation nécessaire", "refresh needed"),
            ("temps réel", "live"),
            ("horaire prévu", "scheduled"),
            ("supprimé", "cancelled"),
        ] {
            result = result.replace(french, english);
        }
        result
    }
    pub fn receive(&mut self, result: Result<Snapshot, String>) {
        match result {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.error.clear();
            }
            Err(error) => {
                self.snapshot = None;
                self.error = error;
            }
        }
    }
    pub fn lines(&self, now: i64) -> Vec<String> {
        let Some(snapshot) = &self.snapshot else {
            return vec![self.translate(&if self.error.is_empty() {
                "Chargement des départs…".into()
            } else {
                self.error.clone()
            })];
        };
        let date = Paris.timestamp_opt(now, 0).unwrap().date_naive();
        let threshold = now;
        let live = snapshot.live_at.is_some_and(|t| fresh(t, now)) && now - snapshot.fetched <= 300;
        let mut departures: Vec<_> = snapshot
            .departures
            .iter()
            .filter_map(|d| {
                let expected = if live { d.expected } else { d.scheduled };
                (expected >= threshold
                    && !(live && d.cancelled)
                    && Paris.timestamp_opt(expected, 0).unwrap().date_naive() == date)
                    .then_some((expected, d))
            })
            .collect();
        departures.sort_by_key(|(expected, _)| *expected);
        let mut lines: Vec<String> = departures
            .into_iter()
            .take(3)
            .enumerate()
            .map(|(index, (expected, d))| {
                let status = if live && d.cancelled {
                    "supprimé".into()
                } else if live && d.live {
                    let delay = ((d.expected - d.scheduled) as f64 / 60.0).round() as i64;
                    if delay == 0 {
                        "temps réel".into()
                    } else {
                        format!("temps réel · {delay:+} min")
                    }
                } else {
                    "horaire prévu".into()
                };
                format!(
                    "{}{} · {} min · {status}",
                    if index == 0 { "Prochain : " } else { "" },
                    Paris.timestamp_opt(expected, 0).unwrap().format("%H:%M"),
                    (expected - now + 59) / 60
                )
            })
            .collect();
        if lines.is_empty() {
            lines.push("Aucun autre départ aujourd’hui après cet horaire.".into());
        }
        lines
            .into_iter()
            .map(|line| self.translate(&line))
            .collect()
    }
    pub fn status(&self) -> String {
        self.translate(&match &self.snapshot {
            Some(snapshot) => {
                let mut status = format!(
                    "Actualisé à {}",
                    Paris
                        .timestamp_opt(snapshot.fetched, 0)
                        .unwrap()
                        .format("%H:%M:%S")
                );
                for note in &snapshot.notices {
                    status.push_str(" · ");
                    status.push_str(note);
                }
                if Utc::now().timestamp() - snapshot.fetched > 300 {
                    status.push_str(" · actualisation nécessaire");
                }
                status
            }
            None => "Bibus / Brest Métropole".into(),
        })
    }
}

pub fn open_source() {
    let _ = Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    })
    .arg(SOURCE)
    .spawn();
}

pub fn icon_rgba() -> Vec<u8> {
    let mut pixels = vec![0; 32 * 32 * 4];
    for y in 3..29 {
        for x in 5..27 {
            let color = if (8..24).contains(&x) && (7..15).contains(&y)
                || (9..12).contains(&x) && (20..23).contains(&y)
                || (20..23).contains(&x) && (20..23).contains(&y)
            {
                if cfg!(target_os = "macos") {
                    [0, 0, 0, 0]
                } else {
                    [255, 255, 255, 255]
                }
            } else {
                if cfg!(target_os = "macos") {
                    [255, 255, 255, 255]
                } else {
                    [23, 107, 84, 255]
                }
            };
            pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(&color);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn english_covers_departures_and_offline_states() {
        let mut model = Model {
            snapshot: None,
            english: true,
            error: String::new(),
        };
        assert_eq!(model.lines(now()), vec!["Loading departures…"]);
        model.receive(Err("Horaires expirés. Réessayez en ligne.".into()));
        assert_eq!(
            model.lines(now()),
            vec!["Timetable expired. Reconnect and retry."]
        );
        model.receive(Ok(Snapshot {
            fetched: now(),
            live_at: None,
            departures: vec![
                departure(300),
                departure(600),
                departure(900),
                departure(1200),
            ],
            notices: vec!["Temps réel indisponible · horaires prévus".into()],
        }));
        let lines = model.lines(now());
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Next: 14:05 · 5 min · scheduled");
        assert!(model.status().starts_with("Updated "));
        assert!(model
            .status()
            .contains("Live data unavailable · scheduled times"));
        assert_eq!(model.lines(now() + 3600), vec!["No more departures today."]);
        model.english = false;
        assert!(model.lines(now())[0].starts_with("Prochain : "));
    }
    fn now() -> i64 {
        Paris
            .with_ymd_and_hms(2026, 11, 3, 14, 0, 0)
            .unwrap()
            .timestamp()
    }
    fn departure(offset: i64) -> Departure {
        Departure {
            trip: "trip".into(),
            date: "20261103".into(),
            sequence: 1,
            scheduled: now() + offset,
            expected: now() + offset,
            live: false,
            cancelled: false,
        }
    }
    fn feed() -> realtime::Feed {
        realtime::Feed {
            header: realtime::Header {
                version: "2.0".into(),
                incrementality: Some(0),
                timestamp: Some(now() as u64),
            },
            entity: vec![realtime::Entity {
                id: "test".into(),
                deleted: None,
                update: Some(realtime::Update {
                    trip: realtime::Trip {
                        id: Some("trip".into()),
                        date: Some("20261103".into()),
                        relationship: None,
                    },
                    timestamp: None,
                    stops: vec![realtime::Stop {
                        sequence: Some(1),
                        id: Some(STOP.into()),
                        relationship: None,
                        departure: Some(realtime::Event {
                            delay: Some(-120),
                            time: None,
                        }),
                    }],
                }),
            }],
        }
    }
    #[test]
    fn negative_delay_is_used() {
        let mut departures = [departure(600)];
        apply_live(&mut departures, &feed().encode_to_vec(), now()).unwrap();
        assert!(departures[0].live);
        assert_eq!(departures[0].expected, now() + 480);
    }
    #[test]
    fn stale_feed_is_rejected() {
        let mut data = feed();
        data.header.timestamp = Some((now() - 301) as u64);
        assert!(apply_live(&mut [departure(600)], &data.encode_to_vec(), now()).is_err());
    }
    #[test]
    fn cancelled_trip_and_skipped_stop() {
        let mut data = feed();
        data.entity[0].update.as_mut().unwrap().trip.relationship = Some(3);
        let mut departures = [departure(600)];
        apply_live(&mut departures, &data.encode_to_vec(), now()).unwrap();
        assert!(departures[0].cancelled);
        data.entity[0].update.as_mut().unwrap().trip.relationship = None;
        data.entity[0].update.as_mut().unwrap().stops[0].relationship = Some(1);
        apply_live(&mut departures, &data.encode_to_vec(), now()).unwrap();
        assert!(departures[0].cancelled);
    }
    #[test]
    fn other_day_and_stale_trip_are_ignored() {
        let mut data = feed();
        data.entity[0].update.as_mut().unwrap().trip.date = Some("20261102".into());
        let mut departures = [departure(600)];
        apply_live(&mut departures, &data.encode_to_vec(), now()).unwrap();
        assert!(!departures[0].live);
        data.entity[0].update.as_mut().unwrap().trip.date = Some("20261103".into());
        data.entity[0].update.as_mut().unwrap().timestamp = Some((now() - 301) as u64);
        apply_live(&mut departures, &data.encode_to_vec(), now()).unwrap();
        assert!(!departures[0].live);
    }
    #[test]
    fn another_stop_is_not_a_prediction() {
        let mut data = feed();
        data.entity[0].update.as_mut().unwrap().stops[0].sequence = Some(2);
        let mut departures = [departure(600)];
        apply_live(&mut departures, &data.encode_to_vec(), now()).unwrap();
        assert!(!departures[0].live);
    }
    #[test]
    fn next_three_from_now_without_1630_cutoff() {
        let mut cancelled = departure(30);
        cancelled.live = true;
        cancelled.cancelled = true;
        let snapshot = Snapshot {
            fetched: now(),
            live_at: Some(now()),
            notices: vec![],
            departures: vec![
                departure(1200),
                departure(-60),
                departure(300),
                cancelled,
                departure(600),
                departure(1800),
            ],
        };
        let model = Model {
            snapshot: Some(snapshot),
            error: String::new(),
            english: false,
        };
        let lines = model.lines(now());
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("Prochain : 14:05"));
        assert!(lines[1].starts_with("14:10"));
        assert!(lines[2].starts_with("14:20"));
        assert!(model.lines(now() + 420)[0].starts_with("Prochain : 14:10"));
    }
    #[test]
    fn expired_predictions_revert_to_schedule() {
        let mut bus = departure(600);
        bus.expected += 300;
        bus.live = true;
        let model = Model {
            snapshot: Some(Snapshot {
                departures: vec![bus],
                fetched: now(),
                live_at: Some(now()),
                notices: vec![],
            }),
            error: String::new(),
            english: false,
        };
        assert!(model.lines(now())[0].starts_with("Prochain : 14:15"));
        let lines = model.lines(now() + 301);
        assert!(lines[0].starts_with("Prochain : 14:10"));
        assert!(lines[0].ends_with("horaire prévu"));
    }
    #[test]
    fn no_departures_and_midnight_do_not_show_previous_day() {
        let model = Model {
            snapshot: Some(Snapshot {
                departures: vec![departure(600)],
                fetched: now(),
                live_at: None,
                notices: vec![],
            }),
            error: String::new(),
            english: false,
        };
        assert_eq!(
            model.lines(now() + 900)[0],
            "Aucun autre départ aujourd’hui après cet horaire."
        );
        assert_eq!(model.lines(now() + 86400).len(), 1);
    }
    #[test]
    fn calendar_exceptions_and_gtfs_after_midnight() {
        let table = Timetable {
            trips: HashMap::from([(
                "trip".into(),
                Trip {
                    trip_id: "trip".into(),
                    route_id: "2".into(),
                    service_id: "holiday".into(),
                    direction_id: "1".into(),
                },
            )]),
            stops: vec![StopTime {
                trip_id: "trip".into(),
                stop_id: STOP.into(),
                stop_sequence: 1,
                departure_time: "25:10:00".into(),
                pickup_type: "0".into(),
            }],
            calendar: vec![],
            exceptions: vec![Exception {
                service_id: "holiday".into(),
                date: "20261102".into(),
                exception_type: "1".into(),
            }],
            first: String::new(),
            last: String::new(),
        };
        let timestamp = Paris
            .with_ymd_and_hms(2026, 11, 3, 0, 30, 0)
            .unwrap()
            .timestamp();
        let buses = table.departures(timestamp);
        assert_eq!(buses.len(), 1);
        assert_eq!(buses[0].date, "20261102");
        assert_eq!(
            Paris
                .timestamp_opt(buses[0].scheduled, 0)
                .unwrap()
                .format("%H:%M")
                .to_string(),
            "01:10"
        );
    }
}

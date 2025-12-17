//! Weather Telemetry App - Fetches real-time weather data and stores in Xtrieve
//!
//! Uses Open-Meteo API (free, no API key required) to fetch weather data
//! for multiple cities and stores observations in a Btrieve-compatible database.

use xtrieve_client::{XtrieveClient, BtrieveRequest};
use std::time::{SystemTime, UNIX_EPOCH};

// Btrieve operation codes
const OP_OPEN: u32 = 0;
const OP_CLOSE: u32 = 1;
const OP_INSERT: u32 = 2;
const OP_CREATE: u32 = 14;
const OP_GET_FIRST: u32 = 12;
const OP_GET_NEXT: u32 = 6;

// Record layout (128 bytes total):
// [0..8]    timestamp (i64, key 0)
// [8..40]   city_name (32 bytes string)
// [40..48]  latitude (f64)
// [48..56]  longitude (f64)
// [56..64]  temperature_c (f64)
// [64..68]  humidity_pct (i32)
// [68..76]  wind_speed_kmh (f64)
// [76..80]  weather_code (i32)
// [80..128] reserved

const RECORD_LEN: usize = 128;

/// Weather observation record
#[derive(Debug, Clone)]
struct WeatherObservation {
    timestamp: i64,
    city_name: String,
    latitude: f64,
    longitude: f64,
    temperature_c: f64,
    humidity_pct: i32,
    wind_speed_kmh: f64,
    weather_code: i32,
}

impl WeatherObservation {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; RECORD_LEN];

        // Timestamp (key)
        buf[0..8].copy_from_slice(&self.timestamp.to_le_bytes());

        // City name (32 bytes, null-padded)
        let city_bytes = self.city_name.as_bytes();
        let copy_len = city_bytes.len().min(32);
        buf[8..8 + copy_len].copy_from_slice(&city_bytes[..copy_len]);

        // Coordinates
        buf[40..48].copy_from_slice(&self.latitude.to_le_bytes());
        buf[48..56].copy_from_slice(&self.longitude.to_le_bytes());

        // Weather data
        buf[56..64].copy_from_slice(&self.temperature_c.to_le_bytes());
        buf[64..68].copy_from_slice(&self.humidity_pct.to_le_bytes());
        buf[68..76].copy_from_slice(&self.wind_speed_kmh.to_le_bytes());
        buf[76..80].copy_from_slice(&self.weather_code.to_le_bytes());

        buf
    }

    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 80 {
            return None;
        }

        let timestamp = i64::from_le_bytes(data[0..8].try_into().ok()?);

        let city_end = data[8..40].iter().position(|&b| b == 0).unwrap_or(32);
        let city_name = String::from_utf8_lossy(&data[8..8 + city_end]).to_string();

        let latitude = f64::from_le_bytes(data[40..48].try_into().ok()?);
        let longitude = f64::from_le_bytes(data[48..56].try_into().ok()?);
        let temperature_c = f64::from_le_bytes(data[56..64].try_into().ok()?);
        let humidity_pct = i32::from_le_bytes(data[64..68].try_into().ok()?);
        let wind_speed_kmh = f64::from_le_bytes(data[68..76].try_into().ok()?);
        let weather_code = i32::from_le_bytes(data[76..80].try_into().ok()?);

        Some(WeatherObservation {
            timestamp,
            city_name,
            latitude,
            longitude,
            temperature_c,
            humidity_pct,
            wind_speed_kmh,
            weather_code,
        })
    }

    fn weather_description(&self) -> &'static str {
        // WMO Weather interpretation codes
        match self.weather_code {
            0 => "Clear sky",
            1 | 2 | 3 => "Partly cloudy",
            45 | 48 => "Foggy",
            51 | 53 | 55 => "Drizzle",
            61 | 63 | 65 => "Rain",
            71 | 73 | 75 => "Snow",
            80 | 81 | 82 => "Rain showers",
            95 => "Thunderstorm",
            96 | 99 => "Thunderstorm with hail",
            _ => "Unknown",
        }
    }
}

/// City with coordinates for weather lookup
struct City {
    name: &'static str,
    lat: f64,
    lon: f64,
}

const CITIES: &[City] = &[
    City { name: "New York", lat: 40.7128, lon: -74.0060 },
    City { name: "London", lat: 51.5074, lon: -0.1278 },
    City { name: "Tokyo", lat: 35.6762, lon: 139.6503 },
    City { name: "Sydney", lat: -33.8688, lon: 151.2093 },
    City { name: "Paris", lat: 48.8566, lon: 2.3522 },
    City { name: "Berlin", lat: 52.5200, lon: 13.4050 },
    City { name: "Moscow", lat: 55.7558, lon: 37.6173 },
    City { name: "Dubai", lat: 25.2048, lon: 55.2708 },
    City { name: "Singapore", lat: 1.3521, lon: 103.8198 },
    City { name: "Sao Paulo", lat: -23.5505, lon: -46.6333 },
];

/// Fetch weather data from Open-Meteo API
async fn fetch_weather(city: &City) -> Result<WeatherObservation, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m",
        city.lat, city.lon
    );

    let response = reqwest::get(&url).await?;
    let json: serde_json::Value = response.json().await?;

    let current = &json["current"];

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    Ok(WeatherObservation {
        timestamp,
        city_name: city.name.to_string(),
        latitude: city.lat,
        longitude: city.lon,
        temperature_c: current["temperature_2m"].as_f64().unwrap_or(0.0),
        humidity_pct: current["relative_humidity_2m"].as_i64().unwrap_or(0) as i32,
        wind_speed_kmh: current["wind_speed_10m"].as_f64().unwrap_or(0.0),
        weather_code: current["weather_code"].as_i64().unwrap_or(0) as i32,
    })
}

/// Build file creation buffer with key spec
fn build_create_buffer() -> Vec<u8> {
    let mut buf = Vec::new();

    // File spec: record_len(2), page_size(2), num_keys(2), unused(4)
    buf.extend_from_slice(&(RECORD_LEN as u16).to_le_bytes()); // record length
    buf.extend_from_slice(&4096u16.to_le_bytes());              // page size
    buf.extend_from_slice(&1u16.to_le_bytes());                 // number of keys
    buf.extend_from_slice(&0u32.to_le_bytes());                 // unused

    // Key spec: position(2), length(2), flags(2), key_type(1), null_val(1), reserved(8)
    buf.extend_from_slice(&0u16.to_le_bytes());    // key position (timestamp at offset 0)
    buf.extend_from_slice(&8u16.to_le_bytes());    // key length (8 bytes for i64)
    buf.extend_from_slice(&1u16.to_le_bytes());    // flags: duplicates allowed
    buf.push(14); // key type: unsigned binary
    buf.push(0);  // null value
    buf.extend_from_slice(&[0u8; 8]); // reserved

    buf
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("===========================================");
    println!("   Weather Telemetry System");
    println!("   Powered by Xtrieve Database Engine");
    println!("===========================================\n");

    // Connect to Xtrieve server
    println!("Connecting to Xtrieve server...");
    let mut client = XtrieveClient::connect("127.0.0.1:7419")?;
    println!("Connected!\n");

    let db_file = "weather.dat";

    // Remove existing file for fresh start
    let _ = std::fs::remove_file(format!("./data/{}", db_file));

    // Create the database file
    println!("Creating weather database...");
    let create_resp = client.execute(BtrieveRequest {
        operation_code: OP_CREATE,
        file_path: db_file.to_string(),
        data_buffer: build_create_buffer(),
        ..Default::default()
    })?;

    if create_resp.status_code != 0 {
        println!("Failed to create database: status {}", create_resp.status_code);
        return Ok(());
    }
    println!("Database created: {}\n", db_file);

    // Open the file
    let open_resp = client.execute(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: db_file.to_string(),
        ..Default::default()
    })?;

    if open_resp.status_code != 0 {
        println!("Failed to open database: status {}", open_resp.status_code);
        return Ok(());
    }
    let pos_block = open_resp.position_block.clone();

    // Fetch weather data for all cities
    println!("Fetching weather data from Open-Meteo API...\n");
    println!("{:<15} {:>8} {:>6} {:>10} {}", "City", "Temp(C)", "Humid%", "Wind km/h", "Conditions");
    println!("{}", "-".repeat(60));

    let mut observations = Vec::new();

    for city in CITIES {
        match fetch_weather(city).await {
            Ok(obs) => {
                println!(
                    "{:<15} {:>8.1} {:>6} {:>10.1} {}",
                    obs.city_name,
                    obs.temperature_c,
                    obs.humidity_pct,
                    obs.wind_speed_kmh,
                    obs.weather_description()
                );
                observations.push(obs);
            }
            Err(e) => {
                println!("{:<15} Error: {}", city.name, e);
            }
        }

        // Small delay to be nice to the API
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\nInserting {} observations into Xtrieve...", observations.len());

    // Insert all observations
    let mut inserted = 0;
    for obs in &observations {
        let insert_resp = client.execute(BtrieveRequest {
            operation_code: OP_INSERT,
            position_block: pos_block.clone(),
            data_buffer: obs.to_bytes(),
            data_buffer_length: RECORD_LEN as u32,
            ..Default::default()
        })?;

        if insert_resp.status_code == 0 {
            inserted += 1;
        } else {
            println!("  Insert failed for {}: status {}", obs.city_name, insert_resp.status_code);
        }
    }
    println!("Inserted {} records.\n", inserted);

    // Read back all records
    println!("Reading all observations from database...\n");
    println!("{:<20} {:<15} {:>8} {:>6} {:>10} {}", "Timestamp", "City", "Temp(C)", "Humid%", "Wind km/h", "Conditions");
    println!("{}", "-".repeat(80));

    // Get first record
    let mut current_pos = pos_block.clone();
    let first_resp = client.execute(BtrieveRequest {
        operation_code: OP_GET_FIRST,
        position_block: current_pos.clone(),
        key_number: 0,
        ..Default::default()
    })?;

    if first_resp.status_code == 0 {
        current_pos = first_resp.position_block.clone();
        if let Some(obs) = WeatherObservation::from_bytes(&first_resp.data_buffer) {
            let datetime = chrono::DateTime::from_timestamp(obs.timestamp, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| obs.timestamp.to_string());

            println!(
                "{:<20} {:<15} {:>8.1} {:>6} {:>10.1} {}",
                datetime,
                obs.city_name,
                obs.temperature_c,
                obs.humidity_pct,
                obs.wind_speed_kmh,
                obs.weather_description()
            );
        }

        // Get remaining records
        loop {
            let next_resp = client.execute(BtrieveRequest {
                operation_code: OP_GET_NEXT,
                position_block: current_pos.clone(),
                key_number: 0,
                ..Default::default()
            })?;

            if next_resp.status_code != 0 {
                break; // End of file or error
            }

            current_pos = next_resp.position_block.clone();

            if let Some(obs) = WeatherObservation::from_bytes(&next_resp.data_buffer) {
                let datetime = chrono::DateTime::from_timestamp(obs.timestamp, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| obs.timestamp.to_string());

                println!(
                    "{:<20} {:<15} {:>8.1} {:>6} {:>10.1} {}",
                    datetime,
                    obs.city_name,
                    obs.temperature_c,
                    obs.humidity_pct,
                    obs.wind_speed_kmh,
                    obs.weather_description()
                );
            }
        }
    }

    // Close file
    println!("\nClosing database...");
    let _ = client.execute(BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: pos_block,
        ..Default::default()
    });

    println!("\n===========================================");
    println!("   Weather telemetry complete!");
    println!("   Data stored in: ./data/{}", db_file);
    println!("===========================================");

    Ok(())
}

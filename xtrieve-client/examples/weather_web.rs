//! Weather Web Server - Displays weather telemetry data from Xtrieve
//!
//! Run this after running weather_telemetry to populate the database.
//! Access at http://localhost:3000

use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use xtrieve_client::{AsyncXtrieveClient, BtrieveRequest};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

// Btrieve operation codes
const OP_OPEN: u32 = 0;
const OP_CLOSE: u32 = 1;
const OP_GET_FIRST: u32 = 12;
const OP_GET_NEXT: u32 = 6;

#[derive(Debug, Clone, Serialize)]
struct WeatherObservation {
    timestamp: i64,
    datetime: String,
    city_name: String,
    latitude: f64,
    longitude: f64,
    temperature_c: f64,
    humidity_pct: i32,
    wind_speed_kmh: f64,
    weather_code: i32,
    weather_description: String,
}

impl WeatherObservation {
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

        let datetime = chrono::DateTime::from_timestamp(timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| timestamp.to_string());

        let weather_description = match weather_code {
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
        }.to_string();

        Some(WeatherObservation {
            timestamp,
            datetime,
            city_name,
            latitude,
            longitude,
            temperature_c,
            humidity_pct,
            wind_speed_kmh,
            weather_code,
            weather_description,
        })
    }
}

type AppState = Arc<Mutex<AsyncXtrieveClient>>;

/// Fetch all weather observations from database
async fn fetch_observations(client: &mut AsyncXtrieveClient) -> Vec<WeatherObservation> {
    let mut observations = Vec::new();

    // Open the file
    let open_resp = match client.execute(BtrieveRequest {
        operation_code: OP_OPEN,
        file_path: "weather.dat".to_string(),
        ..Default::default()
    }).await {
        Ok(r) => r,
        Err(_) => return observations,
    };

    if open_resp.status_code != 0 {
        return observations;
    }

    let pos_block = open_resp.position_block.clone();

    // Get first record
    let first_resp = match client.execute(BtrieveRequest {
        operation_code: OP_GET_FIRST,
        position_block: pos_block.clone(),
        key_number: 0,
        ..Default::default()
    }).await {
        Ok(r) => r,
        Err(_) => {
            let _ = client.execute(BtrieveRequest {
                operation_code: OP_CLOSE,
                position_block: pos_block,
                ..Default::default()
            }).await;
            return observations;
        }
    };

    let mut current_pos = pos_block.clone();

    if first_resp.status_code == 0 {
        current_pos = first_resp.position_block.clone();
        if let Some(obs) = WeatherObservation::from_bytes(&first_resp.data_buffer) {
            observations.push(obs);
        }

        // Get remaining records
        loop {
            let next_resp = match client.execute(BtrieveRequest {
                operation_code: OP_GET_NEXT,
                position_block: current_pos.clone(),
                key_number: 0,
                ..Default::default()
            }).await {
                Ok(r) => r,
                Err(_) => break,
            };

            if next_resp.status_code != 0 {
                break;
            }

            current_pos = next_resp.position_block.clone();

            if let Some(obs) = WeatherObservation::from_bytes(&next_resp.data_buffer) {
                observations.push(obs);
            }
        }
    }

    // Close file
    let _ = client.execute(BtrieveRequest {
        operation_code: OP_CLOSE,
        position_block: pos_block,
        ..Default::default()
    }).await;

    observations
}

/// JSON API endpoint
async fn api_weather(State(state): State<AppState>) -> impl IntoResponse {
    let observations = {
        let mut client = state.lock().await;
        fetch_observations(&mut client).await
    };
    Json(observations)
}

/// HTML dashboard
async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let observations = {
        let mut client = state.lock().await;
        fetch_observations(&mut client).await
    };

    let rows: String = observations.iter().map(|obs| {
        let temp_color = if obs.temperature_c > 30.0 {
            "#ff6b6b"
        } else if obs.temperature_c > 20.0 {
            "#ffd93d"
        } else if obs.temperature_c > 10.0 {
            "#6bcb77"
        } else {
            "#4d96ff"
        };

        format!(
            r#"<tr>
                <td>{}</td>
                <td style="background-color: {}; font-weight: bold;">{:.1}°C</td>
                <td>{}%</td>
                <td>{:.1} km/h</td>
                <td>{}</td>
                <td style="font-size: 0.8em; color: #666;">{:.4}, {:.4}</td>
            </tr>"#,
            obs.city_name,
            temp_color,
            obs.temperature_c,
            obs.humidity_pct,
            obs.wind_speed_kmh,
            obs.weather_description,
            obs.latitude,
            obs.longitude
        )
    }).collect();

    let timestamp = observations.first()
        .map(|o| o.datetime.clone())
        .unwrap_or_else(|| "N/A".to_string());

    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
    <title>Weather Telemetry - Xtrieve Database</title>
    <meta charset="utf-8">
    <meta http-equiv="refresh" content="30">
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        h1 {{
            color: #333;
            border-bottom: 3px solid #4d96ff;
            padding-bottom: 10px;
        }}
        .subtitle {{
            color: #666;
            margin-top: -10px;
            margin-bottom: 20px;
        }}
        .stats {{
            display: flex;
            gap: 20px;
            margin-bottom: 20px;
        }}
        .stat-card {{
            background: white;
            padding: 15px 25px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .stat-value {{
            font-size: 2em;
            font-weight: bold;
            color: #4d96ff;
        }}
        .stat-label {{
            color: #666;
            font-size: 0.9em;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            background: white;
            border-radius: 8px;
            overflow: hidden;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        th {{
            background: #4d96ff;
            color: white;
            padding: 12px 15px;
            text-align: left;
        }}
        td {{
            padding: 12px 15px;
            border-bottom: 1px solid #eee;
        }}
        tr:hover {{
            background: #f8f9fa;
        }}
        .footer {{
            margin-top: 20px;
            color: #666;
            font-size: 0.9em;
            text-align: center;
        }}
        .badge {{
            display: inline-block;
            padding: 3px 8px;
            border-radius: 4px;
            font-size: 0.8em;
            background: #e8f4fd;
            color: #4d96ff;
        }}
    </style>
</head>
<body>
    <h1>Weather Telemetry Dashboard</h1>
    <p class="subtitle">Real-time weather data stored in Xtrieve Database (Async Binary Protocol)</p>

    <div class="stats">
        <div class="stat-card">
            <div class="stat-value">{}</div>
            <div class="stat-label">Cities Monitored</div>
        </div>
        <div class="stat-card">
            <div class="stat-value">{}</div>
            <div class="stat-label">Observations</div>
        </div>
        <div class="stat-card">
            <div class="stat-value">{:.1}°C</div>
            <div class="stat-label">Avg Temperature</div>
        </div>
    </div>

    <table>
        <thead>
            <tr>
                <th>City</th>
                <th>Temperature</th>
                <th>Humidity</th>
                <th>Wind Speed</th>
                <th>Conditions</th>
                <th>Coordinates</th>
            </tr>
        </thead>
        <tbody>
            {}
        </tbody>
    </table>

    <div class="footer">
        <p>Last updated: {} | <span class="badge">Powered by Xtrieve Database Engine</span></p>
        <p>Data source: <a href="https://open-meteo.com/">Open-Meteo API</a> | Auto-refresh every 30 seconds</p>
        <p><a href="/api/weather">JSON API</a></p>
    </div>
</body>
</html>"#,
        observations.len(),
        observations.len(),
        observations.iter().map(|o| o.temperature_c).sum::<f64>() / observations.len().max(1) as f64,
        rows,
        timestamp
    );

    Html(html)
}

/// Health check
async fn health() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("===========================================");
    println!("   Weather Telemetry Web Server");
    println!("   Powered by Xtrieve Database Engine");
    println!("===========================================\n");

    // Connect to Xtrieve server using async client
    println!("Connecting to Xtrieve server...");
    let client = AsyncXtrieveClient::connect("127.0.0.1:7419").await?;
    println!("Connected!\n");

    let state = Arc::new(Mutex::new(client));

    // Build router
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/api/weather", get(api_weather))
        .route("/health", get(health))
        .with_state(state);

    let addr = "0.0.0.0:3000";
    println!("Starting web server on http://{}", addr);
    println!("Open http://localhost:3000 in your browser\n");
    println!("Endpoints:");
    println!("  GET /           - HTML Dashboard");
    println!("  GET /api/weather - JSON API");
    println!("  GET /health     - Health check\n");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

use crate::error::{Error, Result};
use mbn::symbols::Instrument;
use midas_client::historical::Historical;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::fs as async_fs;

fn process_date(date: &str) -> Result<OffsetDateTime> {
    // Append T00:00 to make it full day at 00:00 UTC
    let datetime = format!("{}T00:00:00Z", date);

    // Try to parse the datetime string as an OffsetDateTime
    let datetime = OffsetDateTime::parse(&datetime, &Rfc3339).map_err(|_| {
        Error::DateError(
            "Error: Invalid start date format. Expected format: YYYY-MM-DD".to_string(),
        )
    })?;

    Ok(datetime)
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct JsonTicker {
    pub ticker: String,
    pub name: String,
    pub stype: String,
    pub dataset: String,
    pub first_available: String,
    pub last_update: String,
    pub active: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Ticker {
    pub ticker: String,
    pub name: String,
    pub stype: String,
    pub dataset: String,
    pub first_available: OffsetDateTime,
    pub last_update: OffsetDateTime,
    pub active: bool,
    pub mbn_id: Option<u32>,
}

impl Ticker {
    pub fn new(
        ticker: &str,
        name: &str,
        stype: &str,
        dataset: &str,
        first_available: &str,
        last_update: &str,
        active: bool,
        mbn_id: Option<u32>,
    ) -> Result<Self> {
        Ok(Ticker {
            ticker: ticker.to_string(),
            name: name.to_string(),
            stype: stype.to_string(),
            dataset: dataset.to_string(),
            first_available: process_date(first_available)?,
            last_update: process_date(last_update)?,
            active,
            mbn_id,
        })
    }
    fn to_instrument(&self) -> Instrument {
        Instrument::new(&self.ticker, &self.name, None)
    }
    pub fn get_mbn_id(&self) -> Result<u32> {
        self.mbn_id.ok_or_else(|| {
            // Replace with an appropriate error type and message
            Error::CustomError("mbn_id is missing for this ticker".to_string())
        })
    }
}

impl TryFrom<JsonTicker> for Ticker {
    type Error = Error; // Define your error type

    fn try_from(json_ticker: JsonTicker) -> Result<Self> {
        Ticker::new(
            &json_ticker.ticker,
            &json_ticker.name,
            &json_ticker.stype,
            &json_ticker.dataset,
            &json_ticker.first_available,
            &json_ticker.last_update,
            json_ticker.active,
            None,
        )
    }
}

/// Load the tickers file into HashMap, mirros the json format
/// {"vendor_name" : [JsonTicker, JsonTicker, ...] }
async fn load_tickers_file(config_path: &PathBuf) -> Result<HashMap<String, Vec<JsonTicker>>> {
    if !config_path.exists() {
        panic!(
            "JSON file not found: {}. Please ensure it exists.",
            config_path.display()
        );
    }

    // Read the JSON file as a string asynchronously
    let data: String = async_fs::read_to_string(config_path).await?;

    // Parse the JSON data into a HashMap where the key is a string and the value is a list of tickers
    let json_data: HashMap<String, Vec<JsonTicker>> = serde_json::from_str(&data)?;

    Ok(json_data)
}
/// Creates the a vector of the ACTIVE tickers, for the given source
pub async fn process_tickers(
    tickers: &HashMap<String, Vec<JsonTicker>>,
    source: &str,
) -> Result<Vec<Ticker>> {
    // Get the list of raw tickers for the source
    let raw_tickers = match tickers.get(source) {
        Some(tickers) => tickers,
        None => {
            return Err(Error::TickerLoading(format!(
                "Source '{}' not found",
                source
            )))
        }
    };

    // Initialize a vector to store the successfully converted tickers
    let mut active_tickers = Vec::new();

    // Loop through the tickers and convert active tickers into Ticker using `TryFrom`
    for raw_ticker in raw_tickers {
        if raw_ticker.active {
            match Ticker::try_from(raw_ticker.clone()) {
                Ok(ticker) => active_tickers.push(ticker),
                Err(e) => {
                    // Handle conversion error (e.g., log or return the error)
                    eprintln!("Failed to convert ticker: {:?}", e);
                    return Err(e); // or log the error and continue, based on your needs
                }
            }
        }
    }

    Ok(active_tickers)
}

/// Adds the tickers to the database if not already existing
/// Returns a map of the ticker to id in database
pub async fn create_tickers(tickers: &mut Vec<Ticker>, client: &Historical) -> Result<Vec<Ticker>> {
    for ticker in tickers.iter_mut() {
        let response = client.get_symbol(&ticker.ticker).await?;
        let id: Option<u32> = response.data;

        match id {
            Some(value) => {
                ticker.mbn_id = Some(value);
                // ticker_map.insert(ticker.ticker.clone(), value);
            }
            None => {
                let response = client.create_symbol(&ticker.to_instrument()).await?;
                let id = response.data.ok_or_else(|| {
                    Error::Conversion(format!("Error creating ticker: {}", ticker.ticker.clone()))
                })?;
                ticker.mbn_id = Some(id);
            }
        }
    }
    Ok(tickers.to_vec())
}

pub async fn get_tickers(
    file_path: &PathBuf,
    source: &str,
    client: &Historical,
) -> Result<Vec<Ticker>> {
    // Load File
    let raw_tickers = load_tickers_file(file_path).await?;

    // Process file
    let mut active_tickers = process_tickers(&raw_tickers, source).await?;

    // Create and Map
    let tickers = create_tickers(&mut active_tickers, &client).await?;

    Ok(tickers)
}

#[allow(dead_code)]
fn instruments_to_map(instruments: Vec<Instrument>) -> HashMap<String, u32> {
    instruments
        .into_iter()
        .filter_map(|instrument| instrument.instrument_id.map(|id| (instrument.ticker, id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use serial_test::serial;
    use std::sync::Arc;

    #[tokio::test]
    #[serial]
    async fn test_load_tickers() -> Result<()> {
        // Test
        let tickers = load_tickers_file(&PathBuf::from("tests/tickers.json")).await?;

        // Validate
        assert!(tickers.len() > 0);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_process_tickers() -> Result<()> {
        // Test
        let raw_tickers = load_tickers_file(&PathBuf::from("tests/tickers.json")).await?;
        let active_tickers = process_tickers(&raw_tickers, "databento").await?;

        // Validate
        assert_eq!(active_tickers.len(), 2);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_tickers() -> Result<()> {
        dotenv().ok();
        let client = Arc::new(Historical::new("http://127.0.0.1:8080"));

        // Test
        let raw_tickers = load_tickers_file(&PathBuf::from("tests/tickers.json")).await?;
        let mut active_tickers = process_tickers(&raw_tickers, "databento").await?;
        let tickers = create_tickers(&mut active_tickers, &client).await?;

        // Validate
        for ticker in tickers {
            let id = ticker.get_mbn_id()?;
            assert!(id > 0);
            let _ = client.delete_symbol(&(id as i32)).await?;
        }

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_get_tickers() -> Result<()> {
        let base_url = "http://localhost:8080"; // Update with your actual base URL
        let client = Arc::new(Historical::new(base_url));

        // Test
        let tickers =
            get_tickers(&PathBuf::from("tests/tickers.json"), "databento", &client).await?;

        // Validate
        for ticker in tickers {
            let id = ticker.get_mbn_id()?;
            assert!(id > 0);
            let _ = client.delete_symbol(&(id as i32)).await?;
        }

        Ok(())
    }
}

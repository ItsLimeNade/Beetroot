#[cfg(test)]
pub mod tests {
    use crate::utils::nightscout::{
        Entry, Nightscout, NightscoutError, NightscoutRequestOptions, Trend,
    };
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use serde_json::json;

    #[tokio::test]
    async fn test_get_entry_success() {
        let server = MockServer::start();
        let response = json!([{
            "_id": "abc123",
            "sgv": 120.0,
            "direction": "Flat",
            "date": 1234567890,
            "delta": 1.5,
            "dateString": "2024-06-01T12:00:00Z",
            "mills": 1234567890
        }]);
        let _mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/entries/sgv")
                .query_param("count", "1");
            then.status(200).json_body(response.clone());
        });

        let ns = Nightscout::new();
        let entry = ns.get_entry(&server.url("/")).await.unwrap();
        assert_eq!(entry.sgv, 120.0);
        assert_eq!(entry.direction.as_deref(), Some("Flat"));
        assert_eq!(entry.delta, Some(1.5));
        assert_eq!(entry.date, Some(1234567890));
        assert_eq!(entry.date_string.as_deref(), Some("2024-06-01T12:00:00Z"));
        assert_eq!(entry.mills, Some(1234567890));
    }

    #[tokio::test]
    async fn test_get_entry_no_entries() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/entries/sgv")
                .query_param("count", "1");
            then.status(200).json_body(json!([]));
        });

        let ns = Nightscout::new();
        let err = ns.get_entry(&server.url("/")).await.unwrap_err();
        matches!(err, NightscoutError::NoEntries);
    }

    #[tokio::test]
    async fn test_get_entries_count_option() {
        let server = MockServer::start();
        let response = json!([
            { "_id": "id1", "sgv": 100.0 },
            { "_id": "id2", "sgv": 110.0 }
        ]);
        let _mock = server.mock(|when, then| {
            when.method(GET)
                .path("/api/v1/entries/sgv")
                .query_param("count", "2");
            then.status(200).json_body(response.clone());
        });

        let ns = Nightscout::new();
        let opts = NightscoutRequestOptions::default().count(2);
        let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].sgv, 100.0);
        assert_eq!(entries[1].sgv, 110.0);
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let ns = Nightscout::new();
        let err = ns.get_entry("not a url").await.unwrap_err();
        matches!(err, NightscoutError::Url(_));
    }

    #[tokio::test]
    async fn test_network_error() {
        let ns = Nightscout::new();
        // Port 0 is invalid, should fail to connect
        let err = ns.get_entry("http://127.0.0.1:0/").await.unwrap_err();
        matches!(err, NightscoutError::Network(_));
    }

    #[test]
    fn test_trend() {
        // Test Flat trend
        let entry_flat: Entry = serde_json::from_value(json!({
            "_id": "test1",
            "sgv": 120.0,
            "direction": "Flat"
        }))
        .unwrap();
        assert_eq!(entry_flat.trend(), Trend::Flat);

        // Test DoubleUp
        let entry_up: Entry = serde_json::from_value(json!({
            "_id": "test2",
            "sgv": 120.0,
            "direction": "DoubleUp"
        }))
        .unwrap();
        assert_eq!(entry_up.trend(), Trend::DoubleUp);

        // Test invalid direction defaults to Else
        let entry_invalid: Entry = serde_json::from_value(json!({
            "_id": "test3",
            "sgv": 120.0,
            "direction": "InvalidDirection"
        }))
        .unwrap();
        assert_eq!(entry_invalid.trend(), Trend::Else);

        // Test None direction defaults to Else
        let entry_none: Entry = serde_json::from_value(json!({
            "_id": "test4",
            "sgv": 120.0
        }))
        .unwrap();
        assert_eq!(entry_none.trend(), Trend::Else);
    }
}

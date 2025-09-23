#[cfg(test)]
pub mod tests {
    use crate::utils::nightscout::{
        Delta, Entry, Nightscout, NightscoutError, NightscoutRequestOptions, Trend,
    };
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use serde_json::json;

    // Helper function to create test entries
    fn make_entry(id: &str, sgv: f32, direction: Option<&str>, date_string: Option<&str>) -> Entry {
        let v = serde_json::json!({
            "_id": id,
            "sgv": sgv,
            "direction": direction,
            "dateString": date_string,
        });
        serde_json::from_value(v).unwrap()
    }

    // Group 1: API request tests
    mod api_request_tests {
        use super::*;

        #[tokio::test]
        async fn test_get_entry_success() {
            let server = MockServer::start();
            let response = json!([{
                "_id": "abc123",
                "sgv": 120.0,
                "direction": "Flat",
                "dateString": "2025-09-23T08:38:01.546Z",
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
            assert_eq!(
                entry.date_string.as_deref(),
                Some("2025-09-23T08:38:01.546Z")
            );
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
            let result = ns.get_entry(&server.url("/")).await;
            assert!(matches!(result, Err(NightscoutError::NoEntries)));
        }

        #[tokio::test]
        async fn test_get_entries_count_option() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id2", "sgv": 110.0, "dateString": "2025-09-23T08:38:01.546Z" }
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
            let result = ns.get_entry("not a url").await;
            assert!(matches!(result, Err(NightscoutError::Url(_))));
        }

        #[tokio::test]
        async fn test_network_error() {
            let ns = Nightscout::new();
            // Port 0 is invalid, should fail to connect
            let result = ns.get_entry("http://127.0.0.1:0/").await;
            assert!(matches!(result, Err(NightscoutError::Network(_))));
        }

        #[tokio::test]
        async fn test_get_current_delta() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 120.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id2", "sgv": 110.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id3", "sgv": 105.0, "dateString": "2025-09-23T08:38:01.123B" }
            ]);
            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param("count", "4");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let delta = ns.get_current_delta(&server.url("/")).await.unwrap();
            assert_eq!(delta.value, 10.0); // 120 - 110 = 10
        }
    }

    // Group 2: Entry processing tests
    mod entry_processing_tests {
        use super::*;

        #[test]
        fn test_get_date_id_valid() {
            let entry = make_entry("id", 100.0, None, Some("2025-09-23T08:38:01.546Z"));
            let id = Nightscout::get_date_id(&entry).unwrap();
            assert_eq!(id, "546Z");
        }

        #[test]
        fn test_get_date_id_missing_data() {
            let entry = make_entry("id", 100.0, None, None);
            let result = Nightscout::get_date_id(&entry);
            assert!(matches!(result, Err(NightscoutError::MissingData)));
        }

        #[test]
        fn test_clean_entries_filters_by_date_id() {
            let e1 = make_entry("id1", 100.0, None, Some("2025-09-23T08:38:01.546Z"));
            let e2 = make_entry("id2", 110.0, None, Some("2025-09-23T08:38:01.546Z"));
            let e3 = make_entry("id3", 120.0, None, Some("2025-09-23T08:38:01.789Z"));
            let client = Nightscout::new();
            let filtered = client
                .clean_entries(&vec![e1.clone(), e2.clone(), e3.clone()])
                .unwrap();
            assert_eq!(filtered, vec![e1, e2]);
        }

        #[test]
        fn test_clean_entries_empty_input() {
            let client = Nightscout::new();
            let result = client.clean_entries(&vec![]);
            assert!(matches!(result, Err(NightscoutError::NoEntries)));
        }

        #[test]
        fn test_clean_entries_missing_date_string() {
            let e1 = make_entry("id1", 100.0, None, None);
            let client = Nightscout::new();
            let result = client.clean_entries(&vec![e1]);
            assert!(matches!(result, Err(NightscoutError::MissingData)));
        }

        #[test]
        fn test_entry_get_delta() {
            let e1 = make_entry("id1", 100.0, None, None);
            let e2 = make_entry("id2", 90.0, None, None);
            let delta = e1.get_delta(&e2);
            assert_eq!(delta.value, 10.0);
        }
    }

    // Group 3: Trend and Delta tests
    mod trend_and_delta_tests {
        use super::*;

        #[test]
        fn test_trend_conversion() {
            // Test Flat trend
            let entry_flat = make_entry("test1", 120.0, Some("Flat"), None);
            assert_eq!(entry_flat.trend(), Trend::Flat);

            // Test DoubleUp
            let entry_up = make_entry("test2", 120.0, Some("DoubleUp"), None);
            assert_eq!(entry_up.trend(), Trend::DoubleUp);

            // Test invalid direction defaults to Else
            let entry_invalid = make_entry("test3", 120.0, Some("InvalidDirection"), None);
            assert_eq!(entry_invalid.trend(), Trend::Else);

            // Test None direction defaults to Else
            let entry_none = make_entry("test4", 120.0, None, None);
            assert_eq!(entry_none.trend(), Trend::Else);
        }

        #[test]
        fn test_trend_from_str() {
            assert_eq!(Trend::from("DoubleUp"), Trend::DoubleUp);
            assert_eq!(Trend::from("SingleUp"), Trend::SingleUp);
            assert_eq!(Trend::from("FortyFiveUp"), Trend::FortyFiveUp);
            assert_eq!(Trend::from("Flat"), Trend::Flat);
            assert_eq!(Trend::from("FortyFiveDown"), Trend::FortyFiveDown);
            assert_eq!(Trend::from("SingleDown"), Trend::SingleDown);
            assert_eq!(Trend::from("DoubleDown"), Trend::DoubleDown);
            assert_eq!(Trend::from("Unknown"), Trend::Else);
        }

        #[test]
        fn test_trend_as_arrow() {
            assert_eq!(Trend::DoubleUp.as_arrow(), "↑↑");
            assert_eq!(Trend::SingleUp.as_arrow(), "↑");
            assert_eq!(Trend::FortyFiveUp.as_arrow(), "↗");
            assert_eq!(Trend::Flat.as_arrow(), "→");
            assert_eq!(Trend::FortyFiveDown.as_arrow(), "↘");
            assert_eq!(Trend::SingleDown.as_arrow(), "↓");
            assert_eq!(Trend::DoubleDown.as_arrow(), "↓↓");
            assert_eq!(Trend::Else.as_arrow(), "↮");
        }

        #[test]
        fn test_delta_as_signed_str() {
            let delta = Delta { value: 5.0 };
            assert_eq!(delta.as_signed_str(), "+5");

            let delta = Delta { value: -3.2 };
            assert_eq!(delta.as_signed_str(), "-3.2");

            let delta = Delta { value: 0.0 };
            assert_eq!(delta.as_signed_str(), "+0");
        }
    }

    // Group 4: Configuration and options tests
    mod configuration_tests {
        use super::*;

        #[test]
        fn test_nightscout_request_options() {
            // Default options
            let default_options = NightscoutRequestOptions::default();
            assert_eq!(default_options.count, None);

            // Setting count
            let options = NightscoutRequestOptions::default().count(5);
            assert_eq!(options.count, Some(5));

            // Fluent interface
            let options = NightscoutRequestOptions::default().count(10);
            assert_eq!(options.count, Some(10));
        }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::utils::nightscout::{
        Delta, Entry, Nightscout, NightscoutError, NightscoutRequestOptions, Trend,
    };
    use chrono::{Duration, Utc};
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

    // Helper function to create test entries with date field
    fn make_entry_with_date(
        id: &str,
        sgv: f32,
        direction: Option<&str>,
        date_string: Option<&str>,
        date: Option<u64>,
    ) -> Entry {
        let mut v = serde_json::json!({
            "_id": id,
            "sgv": sgv,
            "direction": direction,
            "dateString": date_string,
        });
        if let Some(date_val) = date {
            v["date"] = json!(date_val);
        }
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
        async fn test_get_entries_hours_back_option() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id2", "sgv": 110.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let opts = NightscoutRequestOptions::default().hours_back(6);
            let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].sgv, 100.0);
            assert_eq!(entries[1].sgv, 110.0);
        }

        #[tokio::test]
        async fn test_get_entries_hours_back_with_count() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id2", "sgv": 110.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]")
                    .query_param("count", "5");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let opts = NightscoutRequestOptions::default().hours_back(6).count(5);
            let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
            assert_eq!(entries.len(), 2);
        }

        #[tokio::test]
        async fn test_get_entries_for_hours() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id2", "sgv": 110.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id3", "sgv": 120.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let entries = ns
                .get_entries_for_hours(&server.url("/"), 12)
                .await
                .unwrap();
            assert_eq!(entries.len(), 3);
            assert_eq!(entries[0].sgv, 100.0);
            assert_eq!(entries[1].sgv, 110.0);
            assert_eq!(entries[2].sgv, 120.0);
        }

        #[tokio::test]
        async fn test_get_entries_for_hours_no_entries() {
            let server = MockServer::start();
            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]");
                then.status(200).json_body(json!([]));
            });

            let ns = Nightscout::new();
            let result = ns.get_entries_for_hours(&server.url("/"), 6).await;
            assert!(matches!(result, Err(NightscoutError::NoEntries)));
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
        fn test_nightscout_request_options_default() {
            // Default options
            let default_options = NightscoutRequestOptions::default();
            assert_eq!(default_options.count, None);
            assert_eq!(default_options.hours_back, None);
        }

        #[test]
        fn test_nightscout_request_options_count() {
            // Setting count
            let options = NightscoutRequestOptions::default().count(5);
            assert_eq!(options.count, Some(5));
            assert_eq!(options.hours_back, None);
        }

        #[test]
        fn test_nightscout_request_options_hours_back() {
            // Setting hours_back
            let options = NightscoutRequestOptions::default().hours_back(6);
            assert_eq!(options.hours_back, Some(6));
            assert_eq!(options.count, None);
        }

        #[test]
        fn test_nightscout_request_options_fluent_interface() {
            // Fluent interface with both options
            let options = NightscoutRequestOptions::default().count(10).hours_back(12);
            assert_eq!(options.count, Some(10));
            assert_eq!(options.hours_back, Some(12));
        }

        #[test]
        fn test_nightscout_request_options_chaining_order() {
            // Test that chaining order doesn't matter
            let options1 = NightscoutRequestOptions::default().count(5).hours_back(3);
            let options2 = NightscoutRequestOptions::default().hours_back(3).count(5);

            assert_eq!(options1.count, options2.count);
            assert_eq!(options1.hours_back, options2.hours_back);
        }
    }

    // Group 5: Time-based query URL construction tests
    mod time_query_tests {
        use super::*;
        use chrono::{Duration, Utc};

        #[tokio::test]
        async fn test_hours_back_query_parameters() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            // Calculate expected timestamps for verification
            let now = Utc::now();
            let hours_ago = now - Duration::hours(6);
            let start_timestamp = hours_ago.timestamp_millis() as u64;
            let end_timestamp = now.timestamp_millis() as u64;

            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    // We can't predict exact timestamps, so we just verify the parameters exist
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let opts = NightscoutRequestOptions::default().hours_back(6);
            let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
            assert_eq!(entries.len(), 1);
        }

        #[tokio::test]
        async fn test_hours_back_precedence_over_count_in_url() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            // When both hours_back and count are set, it should use the time-based query
            // but still include the count parameter
            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]")
                    .query_param("count", "10");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let opts = NightscoutRequestOptions::default().hours_back(6).count(10);
            let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
            assert_eq!(entries.len(), 1);
        }

        #[tokio::test]
        async fn test_count_only_query_still_works() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" },
                { "_id": "id2", "sgv": 110.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            // When only count is set (no hours_back), should use the original count-based query
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
        }
    }

    // Group 6: Edge cases and error handling for time-based queries
    mod time_query_edge_cases {
        use super::*;

        #[tokio::test]
        async fn test_zero_hours_back() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let opts = NightscoutRequestOptions::default().hours_back(0);
            let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
            assert_eq!(entries.len(), 1);
        }

        #[tokio::test]
        async fn test_large_hours_back_value() {
            let server = MockServer::start();
            let response = json!([
                { "_id": "id1", "sgv": 100.0, "dateString": "2025-09-23T08:38:01.546Z" }
            ]);

            let _mock = server.mock(|when, then| {
                when.method(GET)
                    .path("/api/v1/entries/sgv")
                    .query_param_exists("find[date][$gte]")
                    .query_param_exists("find[date][$lte]");
                then.status(200).json_body(response.clone());
            });

            let ns = Nightscout::new();
            let opts = NightscoutRequestOptions::default().hours_back(255); // Max u8 value
            let entries = ns.get_entries(&server.url("/"), opts).await.unwrap();
            assert_eq!(entries.len(), 1);
        }

        #[tokio::test]
        async fn test_get_entries_for_hours_invalid_url() {
            let ns = Nightscout::new();
            let result = ns.get_entries_for_hours("not a valid url", 6).await;
            assert!(matches!(result, Err(NightscoutError::Url(_))));
        }

        #[tokio::test]
        async fn test_get_entries_for_hours_network_error() {
            let ns = Nightscout::new();
            let result = ns.get_entries_for_hours("http://127.0.0.1:0/", 6).await;
            assert!(matches!(result, Err(NightscoutError::Network(_))));
        }
    }
}

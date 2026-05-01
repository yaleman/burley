use opentelemetry::{global, metrics::MeterProvider};
use opentelemetry_sdk::{Resource, metrics::SdkMeterProvider};

pub fn init() -> opentelemetry_sdk::metrics::SdkMeterProvider {
    let exporter = opentelemetry_stdout::MetricExporterBuilder::default()
        // Build exporter using Delta Temporality (Defaults to Temporality::Cumulative)
        // .with_temporality(opentelemetry_sdk::metrics::Temporality::Delta)
        .build();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name(env!("CARGO_PKG_NAME"))
                .build(),
        )
        .build();
    global::set_meter_provider(provider.clone());
    provider
}

pub struct StatsMeters {
    pub tx_bytes: opentelemetry::metrics::Counter<u64>,
    pub rx_bytes: opentelemetry::metrics::Counter<u64>,
    pub cache_size: opentelemetry::metrics::Gauge<u64>,
}

pub fn init_meters(provider: &opentelemetry_sdk::metrics::SdkMeterProvider) -> StatsMeters {
    let meter = provider.meter(env!("CARGO_PKG_NAME"));
    StatsMeters {
        tx_bytes: meter
            .u64_counter("tx_bytes")
            .with_description("Total bytes sent")
            .build(),
        rx_bytes: meter
            .u64_counter("rx_bytes")
            .with_description("Total bytes received")
            .build(),
        cache_size: meter
            .u64_gauge("cache_size")
            .with_description("Current cache size in bytes")
            .build(),
    }
}

#[tokio::test]
async fn test_init() {
    init_meters(&init());
}

//! Analytics module for trend analysis and insights
//! Issue #8: Trend Analysis & Notification Features

pub mod keyword_trends;
pub mod entity_trends;

pub use keyword_trends::{KeywordTrend, KeywordTrendAnalyzer, TrendDirection, TimeSeriesPoint};
pub use entity_trends::{EntityTrend, EntityTrendAnalyzer, EntityCooccurrence};

//! Keyword trend analysis with time-series tracking and spike detection
//!
//! This module provides functionality for:
//! - Tracking keyword frequencies over time
//! - Detecting trending keywords using moving averages
//! - Identifying spikes in keyword usage
//! - Computing trend direction and velocity

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

/// Errors that can occur during keyword trend analysis
#[derive(Debug, Error)]
pub enum TrendError {
    #[error("Insufficient data points: need at least {0}, got {1}")]
    InsufficientData(usize, usize),

    #[error("Invalid time range: start {0} is after end {1}")]
    InvalidTimeRange(DateTime<Utc>, DateTime<Utc>),

    #[error("Invalid window size: {0}")]
    InvalidWindowSize(usize),

    #[error("Keyword not found: {0}")]
    KeywordNotFound(String),
}

/// Result type for trend analysis operations
pub type TrendResult<T> = Result<T, TrendError>;

/// Time-series data point for a keyword
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Timestamp of this data point
    pub timestamp: DateTime<Utc>,

    /// Frequency count at this timestamp
    pub count: u64,

    /// Normalized frequency (0.0 to 1.0)
    pub normalized: f64,
}

impl DataPoint {
    /// Create a new data point
    #[must_use]
    pub fn new(timestamp: DateTime<Utc>, count: u64) -> Self {
        Self {
            timestamp,
            count,
            normalized: 0.0,
        }
    }
}

/// Trend direction indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    /// Strongly increasing
    Rising,

    /// Slightly increasing
    SlightlyRising,

    /// Stable, no significant change
    Stable,

    /// Slightly decreasing
    SlightlyFalling,

    /// Strongly decreasing
    Falling,
}

impl TrendDirection {
    /// Determine trend direction from velocity
    ///
    /// # Arguments
    /// * `velocity` - Rate of change (-1.0 to 1.0)
    ///
    /// # Classification
    /// - `velocity > 0.3`: Rising
    /// - `0.1 < velocity <= 0.3`: SlightlyRising
    /// - `-0.1 <= velocity <= 0.1`: Stable
    /// - `-0.3 <= velocity < -0.1`: SlightlyFalling
    /// - `velocity < -0.3`: Falling
    #[must_use]
    pub fn from_velocity(velocity: f64) -> Self {
        if velocity > 0.3 {
            Self::Rising
        } else if velocity > 0.1 {
            Self::SlightlyRising
        } else if velocity >= -0.1 {
            Self::Stable
        } else if velocity >= -0.3 {
            Self::SlightlyFalling
        } else {
            Self::Falling
        }
    }
}

/// Spike detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spike {
    /// When the spike occurred
    pub timestamp: DateTime<Utc>,

    /// Actual count at spike
    pub count: u64,

    /// Expected count (moving average)
    pub expected: f64,

    /// Spike magnitude (count / expected)
    pub magnitude: f64,

    /// Z-score of the spike
    pub z_score: f64,
}

/// Keyword time-series with trend analysis capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordTrend {
    /// Keyword being tracked
    pub keyword: String,

    /// Time-series data points (sorted by timestamp)
    data: BTreeMap<DateTime<Utc>, DataPoint>,

    /// Moving average window size (in number of data points)
    window_size: usize,

    /// Cached moving average values
    #[serde(skip)]
    moving_avg_cache: HashMap<DateTime<Utc>, f64>,
}

impl KeywordTrend {
    /// Create a new keyword trend tracker
    ///
    /// # Arguments
    /// * `keyword` - The keyword to track
    /// * `window_size` - Number of data points for moving average (default: 7)
    #[must_use]
    pub fn new(keyword: String, window_size: Option<usize>) -> Self {
        Self {
            keyword,
            data: BTreeMap::new(),
            window_size: window_size.unwrap_or(7),
            moving_avg_cache: HashMap::new(),
        }
    }

    /// Add a data point to the time series
    ///
    /// # Arguments
    /// * `timestamp` - When the count was recorded
    /// * `count` - Frequency count at this time
    pub fn add_point(&mut self, timestamp: DateTime<Utc>, count: u64) {
        let point = DataPoint::new(timestamp, count);
        self.data.insert(timestamp, point);
        self.moving_avg_cache.clear(); // Invalidate cache
    }

    /// Add multiple data points at once
    ///
    /// # Arguments
    /// * `points` - Iterator of (timestamp, count) tuples
    pub fn add_points<I>(&mut self, points: I)
    where
        I: IntoIterator<Item = (DateTime<Utc>, u64)>,
    {
        for (timestamp, count) in points {
            self.add_point(timestamp, count);
        }
    }

    /// Get the number of data points
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if there are no data points
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get all data points in chronological order
    #[must_use]
    pub fn data_points(&self) -> Vec<&DataPoint> {
        self.data.values().collect()
    }

    /// Get data points within a time range
    ///
    /// # Arguments
    /// * `start` - Start of time range (inclusive)
    /// * `end` - End of time range (inclusive)
    #[must_use]
    pub fn data_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&DataPoint> {
        self.data
            .range(start..=end)
            .map(|(_, point)| point)
            .collect()
    }

    /// Calculate simple moving average for a specific timestamp
    ///
    /// # Arguments
    /// * `timestamp` - The timestamp to calculate average for
    ///
    /// # Returns
    /// Moving average value, or None if insufficient data
    #[must_use]
    pub fn moving_average(&mut self, timestamp: DateTime<Utc>) -> Option<f64> {
        // Check cache first
        if let Some(&cached) = self.moving_avg_cache.get(&timestamp) {
            return Some(cached);
        }

        // Get position of timestamp
        let timestamps: Vec<_> = self.data.keys().copied().collect();
        let pos = timestamps.iter().position(|&t| t == timestamp)?;

        // Calculate window boundaries
        let start_idx = pos.saturating_sub(self.window_size - 1);
        let window_data: Vec<_> = timestamps[start_idx..=pos]
            .iter()
            .filter_map(|t| self.data.get(t))
            .collect();

        if window_data.is_empty() {
            return None;
        }

        // Calculate average
        let sum: u64 = window_data.iter().map(|p| p.count).sum();
        let avg = sum as f64 / window_data.len() as f64;

        // Cache result
        self.moving_avg_cache.insert(timestamp, avg);

        Some(avg)
    }

    /// Detect spikes in the time series
    ///
    /// A spike is detected when a data point significantly exceeds the moving average.
    ///
    /// # Arguments
    /// * `threshold` - Minimum magnitude for spike detection (e.g., 2.0 = 2x moving average)
    ///
    /// # Returns
    /// Vector of detected spikes
    pub fn detect_spikes(&mut self, threshold: f64) -> Vec<Spike> {
        let mut spikes = Vec::new();

        // Need enough data for moving average
        if self.data.len() < self.window_size {
            return spikes;
        }

        // Calculate mean and standard deviation for z-scores
        let counts: Vec<f64> = self.data.values().map(|p| p.count as f64).collect();
        let mean = counts.iter().sum::<f64>() / counts.len() as f64;
        let variance = counts.iter().map(|&x| (x - mean).powi(2)).sum::<f64>()
            / counts.len() as f64;
        let std_dev = variance.sqrt();

        // Collect timestamps and counts first to avoid borrow issues
        let data_snapshot: Vec<_> = self.data
            .iter()
            .map(|(&ts, p)| (ts, p.count))
            .collect();

        // Check each point
        for (timestamp, count) in data_snapshot {
            if let Some(expected) = self.moving_average(timestamp) {
                let magnitude = count as f64 / expected;

                if magnitude >= threshold {
                    let z_score = if std_dev > 0.0 {
                        (count as f64 - mean) / std_dev
                    } else {
                        0.0
                    };

                    spikes.push(Spike {
                        timestamp,
                        count,
                        expected,
                        magnitude,
                        z_score,
                    });
                }
            }
        }

        spikes
    }

    /// Calculate trend direction and velocity
    ///
    /// Velocity is calculated using linear regression on recent data points.
    ///
    /// # Arguments
    /// * `recent_points` - Number of recent points to consider (default: window_size)
    ///
    /// # Returns
    /// `(direction, velocity)` where velocity is normalized rate of change
    pub fn trend_direction(&self, recent_points: Option<usize>) -> TrendResult<(TrendDirection, f64)> {
        let n = recent_points.unwrap_or(self.window_size);

        if self.data.len() < 2 {
            return Err(TrendError::InsufficientData(2, self.data.len()));
        }

        // Get recent data points
        let recent: Vec<_> = self.data.iter().rev().take(n).collect();

        if recent.len() < 2 {
            return Err(TrendError::InsufficientData(2, recent.len()));
        }

        // Calculate linear regression slope
        let points: Vec<(f64, f64)> = recent
            .iter()
            .rev()
            .enumerate()
            .map(|(i, (_, point))| (i as f64, point.count as f64))
            .collect();

        let n_f64 = points.len() as f64;
        let sum_x: f64 = points.iter().map(|(x, _)| x).sum();
        let sum_y: f64 = points.iter().map(|(_, y)| y).sum();
        let sum_xy: f64 = points.iter().map(|(x, y)| x * y).sum();
        let sum_x2: f64 = points.iter().map(|(x, _)| x * x).sum();

        let slope = (n_f64 * sum_xy - sum_x * sum_y) / (n_f64 * sum_x2 - sum_x * sum_x);

        // Normalize velocity to [-1.0, 1.0] based on mean
        let mean = sum_y / n_f64;
        let velocity = if mean > 0.0 {
            (slope / mean).clamp(-1.0, 1.0)
        } else {
            0.0
        };

        let direction = TrendDirection::from_velocity(velocity);

        Ok((direction, velocity))
    }

    /// Calculate percent change between two timestamps
    ///
    /// # Arguments
    /// * `start` - Earlier timestamp
    /// * `end` - Later timestamp
    ///
    /// # Returns
    /// Percentage change from start to end
    pub fn percent_change(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> TrendResult<f64> {
        if start >= end {
            return Err(TrendError::InvalidTimeRange(start, end));
        }

        let start_point = self.data.get(&start)
            .ok_or_else(|| TrendError::KeywordNotFound(format!("No data at {start}")))?;
        let end_point = self.data.get(&end)
            .ok_or_else(|| TrendError::KeywordNotFound(format!("No data at {end}")))?;

        if start_point.count == 0 {
            return Ok(100.0); // Arbitrary high value for zero baseline
        }

        let change = ((end_point.count as f64 - start_point.count as f64)
            / start_point.count as f64) * 100.0;

        Ok(change)
    }

    /// Normalize all data points to [0.0, 1.0] range
    ///
    /// Modifies the `normalized` field of each data point based on min-max normalization.
    pub fn normalize(&mut self) {
        if self.data.is_empty() {
            return;
        }

        let counts: Vec<u64> = self.data.values().map(|p| p.count).collect();
        let min = *counts.iter().min().unwrap_or(&0);
        let max = *counts.iter().max().unwrap_or(&0);

        let range = max - min;

        if range == 0 {
            // All values are the same
            for point in self.data.values_mut() {
                point.normalized = 0.5;
            }
        } else {
            for point in self.data.values_mut() {
                point.normalized = (point.count - min) as f64 / range as f64;
            }
        }
    }
}

/// Collection of keyword trends for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalyzer {
    /// Map of keyword to its trend data
    trends: HashMap<String, KeywordTrend>,

    /// Default window size for new trends
    default_window_size: usize,
}

impl TrendAnalyzer {
    /// Create a new trend analyzer
    ///
    /// # Arguments
    /// * `window_size` - Default window size for moving averages
    #[must_use]
    pub fn new(window_size: Option<usize>) -> Self {
        Self {
            trends: HashMap::new(),
            default_window_size: window_size.unwrap_or(7),
        }
    }

    /// Add or update a keyword trend
    ///
    /// # Arguments
    /// * `keyword` - The keyword
    /// * `timestamp` - When the count was recorded
    /// * `count` - Frequency count
    pub fn add_observation(&mut self, keyword: &str, timestamp: DateTime<Utc>, count: u64) {
        self.trends
            .entry(keyword.to_string())
            .or_insert_with(|| KeywordTrend::new(keyword.to_string(), Some(self.default_window_size)))
            .add_point(timestamp, count);
    }

    /// Get a keyword trend
    #[must_use]
    pub fn get_trend(&self, keyword: &str) -> Option<&KeywordTrend> {
        self.trends.get(keyword)
    }

    /// Get a mutable keyword trend
    pub fn get_trend_mut(&mut self, keyword: &str) -> Option<&mut KeywordTrend> {
        self.trends.get_mut(keyword)
    }

    /// Get all keywords being tracked
    #[must_use]
    pub fn keywords(&self) -> Vec<&str> {
        self.trends.keys().map(String::as_str).collect()
    }

    /// Find top trending keywords based on recent velocity
    ///
    /// # Arguments
    /// * `limit` - Maximum number of keywords to return
    /// * `recent_points` - Number of recent points for trend calculation
    ///
    /// # Returns
    /// Keywords sorted by velocity (descending)
    pub fn top_trending(&mut self, limit: usize, recent_points: Option<usize>) -> Vec<(String, f64)> {
        let mut velocities: Vec<_> = self.trends
            .iter_mut()
            .filter_map(|(keyword, trend)| {
                trend.trend_direction(recent_points)
                    .ok()
                    .map(|(_, velocity)| (keyword.clone(), velocity))
            })
            .collect();

        velocities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        velocities.truncate(limit);
        velocities
    }

    /// Find keywords with recent spikes
    ///
    /// # Arguments
    /// * `threshold` - Minimum magnitude for spike detection
    /// * `since` - Only consider spikes after this timestamp
    ///
    /// # Returns
    /// Map of keyword to its spikes
    pub fn find_spikes(
        &mut self,
        threshold: f64,
        since: Option<DateTime<Utc>>,
    ) -> HashMap<String, Vec<Spike>> {
        let cutoff = since.unwrap_or_else(|| Utc::now() - Duration::days(7));

        self.trends
            .iter_mut()
            .filter_map(|(keyword, trend)| {
                let spikes: Vec<_> = trend
                    .detect_spikes(threshold)
                    .into_iter()
                    .filter(|spike| spike.timestamp >= cutoff)
                    .collect();

                if spikes.is_empty() {
                    None
                } else {
                    Some((keyword.clone(), spikes))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_trend_basic() {
        let mut trend = KeywordTrend::new("rust".to_string(), Some(3));
        let now = Utc::now();

        trend.add_point(now, 10);
        trend.add_point(now + Duration::hours(1), 15);
        trend.add_point(now + Duration::hours(2), 20);

        assert_eq!(trend.len(), 3);
        assert!(!trend.is_empty());
    }

    #[test]
    fn test_moving_average() {
        let mut trend = KeywordTrend::new("test".to_string(), Some(3));
        let now = Utc::now();

        trend.add_point(now, 10);
        trend.add_point(now + Duration::hours(1), 20);
        trend.add_point(now + Duration::hours(2), 30);

        let avg = trend.moving_average(now + Duration::hours(2)).unwrap();
        assert!((avg - 20.0).abs() < 0.01); // (10 + 20 + 30) / 3 = 20
    }

    #[test]
    fn test_trend_direction() {
        let mut trend = KeywordTrend::new("rising".to_string(), Some(3));
        let now = Utc::now();

        // Upward trend
        trend.add_point(now, 10);
        trend.add_point(now + Duration::hours(1), 20);
        trend.add_point(now + Duration::hours(2), 30);

        let (direction, velocity) = trend.trend_direction(None).unwrap();
        assert!(velocity > 0.0);
        assert!(matches!(direction, TrendDirection::Rising | TrendDirection::SlightlyRising));
    }

    #[test]
    fn test_spike_detection() {
        let mut trend = KeywordTrend::new("spike".to_string(), Some(3));
        let now = Utc::now();

        // Normal pattern with one spike
        trend.add_point(now, 10);
        trend.add_point(now + Duration::hours(1), 12);
        trend.add_point(now + Duration::hours(2), 11);
        trend.add_point(now + Duration::hours(3), 50); // Spike!
        trend.add_point(now + Duration::hours(4), 10);

        let spikes = trend.detect_spikes(2.0);
        assert!(!spikes.is_empty());
        assert!(spikes[0].magnitude >= 2.0);
    }

    #[test]
    fn test_trend_analyzer() {
        let mut analyzer = TrendAnalyzer::new(Some(3));
        let now = Utc::now();

        analyzer.add_observation("rust", now, 10);
        analyzer.add_observation("rust", now + Duration::hours(1), 20);
        analyzer.add_observation("python", now, 5);

        assert_eq!(analyzer.keywords().len(), 2);
        assert!(analyzer.get_trend("rust").is_some());
        assert!(analyzer.get_trend("python").is_some());
    }

    #[test]
    fn test_normalize() {
        let mut trend = KeywordTrend::new("test".to_string(), Some(3));
        let now = Utc::now();

        trend.add_point(now, 10);
        trend.add_point(now + Duration::hours(1), 50);
        trend.add_point(now + Duration::hours(2), 30);

        trend.normalize();

        let points = trend.data_points();
        assert!((points[0].normalized - 0.0).abs() < 0.01); // min
        assert!((points[1].normalized - 1.0).abs() < 0.01); // max
        assert!((points[2].normalized - 0.5).abs() < 0.01); // middle
    }
}

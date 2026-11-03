//! The subset of the official GTFS-RT protobuf used here. Unknown fields are skipped.
//! Field numbers: https://github.com/google/transit/blob/master/gtfs-realtime/proto/gtfs-realtime.proto
#[derive(Clone, PartialEq, prost::Message)]
pub struct Feed {
    #[prost(message, required, tag = "1")]
    pub header: Header,
    #[prost(message, repeated, tag = "2")]
    pub entity: Vec<Entity>,
}
#[derive(Clone, PartialEq, prost::Message)]
pub struct Header {
    #[prost(string, required, tag = "1")]
    pub version: String,
    #[prost(int32, optional, tag = "2")]
    pub incrementality: Option<i32>,
    #[prost(uint64, optional, tag = "3")]
    pub timestamp: Option<u64>,
}
#[derive(Clone, PartialEq, prost::Message)]
pub struct Entity {
    #[prost(string, required, tag = "1")]
    pub id: String,
    #[prost(bool, optional, tag = "2")]
    pub deleted: Option<bool>,
    #[prost(message, optional, tag = "3")]
    pub update: Option<Update>,
}
#[derive(Clone, PartialEq, prost::Message)]
pub struct Update {
    #[prost(message, required, tag = "1")]
    pub trip: Trip,
    #[prost(message, repeated, tag = "2")]
    pub stops: Vec<Stop>,
    #[prost(uint64, optional, tag = "4")]
    pub timestamp: Option<u64>,
}
#[derive(Clone, PartialEq, prost::Message)]
pub struct Trip {
    #[prost(string, optional, tag = "1")]
    pub id: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub date: Option<String>,
    #[prost(int32, optional, tag = "4")]
    pub relationship: Option<i32>,
}
#[derive(Clone, PartialEq, prost::Message)]
pub struct Stop {
    #[prost(uint32, optional, tag = "1")]
    pub sequence: Option<u32>,
    #[prost(message, optional, tag = "3")]
    pub departure: Option<Event>,
    #[prost(string, optional, tag = "4")]
    pub id: Option<String>,
    #[prost(int32, optional, tag = "5")]
    pub relationship: Option<i32>,
}
#[derive(Clone, PartialEq, prost::Message)]
pub struct Event {
    #[prost(int32, optional, tag = "1")]
    pub delay: Option<i32>,
    #[prost(int64, optional, tag = "2")]
    pub time: Option<i64>,
}

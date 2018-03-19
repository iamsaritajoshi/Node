// Copyright (c) 2017-2018, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.
use std::net::SocketAddr;
use cryptde::PlainData;
use std::marker::Send;
use actix::Subscriber;
use hopper::ExpiredCoresPackage;
use peer_actors::BindMessage;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ClientResponsePayload {
    pub stream_key: SocketAddr,
    pub last_response: bool,
    pub data: PlainData
}

#[derive(Clone)]
pub struct ProxyClientSubs {
    pub bind: Box<Subscriber<BindMessage> + Send>,
    pub from_hopper: Box<Subscriber<ExpiredCoresPackage> + Send>,
}

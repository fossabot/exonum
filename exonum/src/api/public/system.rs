// Copyright 2017 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use router::Router;
use iron::prelude::*;

use helpers::Height;
use node::state::TxPool;
use blockchain::SharedNodeState;
use blockchain::Blockchain;
use crypto::{Hash, HexValue};
use events::Milliseconds;
use explorer::{TxInfo, BlockchainExplorer};
use api::{Api, ApiError};

#[derive(Serialize)]
struct MemPoolTxInfo {
    content: ::serde_json::Value,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum MemPoolResult {
    Unknown,
    MemPool(MemPoolTxInfo),
    Committed(TxInfo),
}

#[derive(Serialize)]
struct MemPoolInfo {
    size: usize,
}

#[derive(Serialize)]
struct HealthStatus {
    height: Height,
    pool_size: usize,
    height_timeout: Option<Milliseconds>,
}

/// Public system API.
#[derive(Clone, Debug)]
pub struct SystemApi {
    pool: TxPool,
    blockchain: Blockchain,
    shared_api_state: SharedNodeState,
}

impl SystemApi {
    /// Creates a new `private::SystemApi` instance.
    pub fn new(pool: TxPool,
               blockchain: Blockchain,
               shared_api_state: SharedNodeState) -> SystemApi {
        SystemApi { pool, blockchain, shared_api_state }
    }

    fn get_mempool_info(&self) -> MemPoolInfo {
        MemPoolInfo { size: self.pool.read().expect("Expected read lock").len() }
    }

    fn get_transaction(&self, hash_str: &str) -> Result<MemPoolResult, ApiError> {
        let hash = Hash::from_hex(hash_str)?;
        self.pool
            .read()
            .expect("Expected read lock")
            .get(&hash)
            .map_or_else(
                || {
                    let explorer = BlockchainExplorer::new(&self.blockchain);
                    Ok(explorer.tx_info(&hash)?.map_or(
                        MemPoolResult::Unknown,
                        MemPoolResult::Committed,
                    ))
                },
                |o| Ok(MemPoolResult::MemPool(MemPoolTxInfo { content: o.info() })),
            )
    }
    fn get_health_status(&self) -> HealthStatus {
        HealthStatus {
            height: self.shared_api_state.height(),
            height_timeout: self.shared_api_state.height_timeout(),
            pool_size: self.shared_api_state.pool_size(),
        }
    }
}

impl Api for SystemApi {
    fn wire(&self, router: &mut Router) {
        let _self = self.clone();
        let mempool_info = move |_: &mut Request| -> IronResult<Response> {
            let info = _self.get_mempool_info();
            _self.ok_response(&::serde_json::to_value(info).unwrap())
        };

        let _self = self.clone();
        let transaction = move |req: &mut Request| -> IronResult<Response> {
            let params = req.extensions.get::<Router>().unwrap();
            match params.find("hash") {
                Some(hash_str) => {
                    let info = _self.get_transaction(hash_str)?;
                    let result = match info {
                        MemPoolResult::Unknown => Self::not_found_response,
                        _ => Self::ok_response,
                    };
                    result(&_self, &::serde_json::to_value(info).unwrap())
                }
                None => {
                    Err(ApiError::IncorrectRequest(
                        "Required parameter of transaction 'hash' is missing".into(),
                    ))?
                }
            }
        };

        let _self = self.clone();
        let health = move |_: &mut Request| -> IronResult<Response> {
            let info = _self.get_health_status();
            _self.ok_response(&::serde_json::to_value(info).unwrap())
        };

        router.get("/v1/health", health, "health");
        router.get("/v1/mempool", mempool_info, "mempool");
        router.get("/v1/transactions/:hash", transaction, "hash");
    }
}

//
// Copyright 2019 The Project Oak Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! Private Set Intersection example for Project Oak.
//!
//! Clients invoke the module by providing their own private set, and the module keeps track of the
//! intersection of all the provided sets from all the clients that have interacted with it.
//! The number of contributed private sets is limited and defined by `SET_THRESHOLD`.
//!
//! The (common) intersection can then be retrieved by each client by a separate invocation.
//! After the first client retrieves the intersection it becomes locked, and new contributions are
//! discarded.
//!
//! Each client request should be provided with a set ID. This is necessary for allowing multiple
//! sets of clients to compute their own intersections.
//!
//! It's important to note that in the current implementation of the application labels, specifying
//! a different set ID does not provide guarantees that data from different clients is kept
//! separate.

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/oak.examples.private_set_intersection.rs"
    ));
}

pub mod handler;

use crate::handler::Handler;
use anyhow::Context;
use oak::{
    io::{forward_invocation, Sender},
    Label,
};
use oak_abi::{
    label::{confidentiality_label, web_assembly_module_signature_tag},
    proto::oak::application::ConfigMap,
};
use oak_services::proto::oak::log::LogInit;

// Base64 encoded Ed25519 public key corresponding to Wasm module signature.
// Originated from `examples/keys/ed25519/test.pub`.
const PUBLIC_KEY_BASE64: &str = "f41SClNtR4i46v2Tuh1fQLbt/ZqRr1lENajCW92jyP4=";

oak::entrypoint_command_handler!(oak_main => Main);

#[derive(Default)]
struct Main;

impl oak::CommandHandler for Main {
    type Command = ConfigMap;

    fn handle_command(&mut self, _command: ConfigMap) -> anyhow::Result<()> {
        let log_sender = oak::logger::create()?;
        oak::logger::init(log_sender.clone(), log::Level::Debug)?;
        let router_command_sender = oak::io::entrypoint_node_create::<Router, _, _>(
            "router",
            &Label::public_untrusted(),
            "app",
            LogInit {
                log_sender: Some(log_sender),
            },
        )
        .context("Couldn't create router node")?;

        oak::grpc::server::init_with_sender("[::]:8080", router_command_sender)
            .context("Couldn't create gRPC server pseudo-Node")?;
        Ok(())
    }
}

oak::entrypoint_command_handler_init!(router => Router);

struct Router {
    /// Invocation sender channel half for Handler Node.
    handler_command_sender: Sender<oak::grpc::Invocation>,
}

impl oak::WithInit for Router {
    type Init = LogInit;

    fn create(init: Self::Init) -> Self {
        let log_sender = init.log_sender.unwrap();
        oak::logger::init(log_sender.clone(), log::Level::Debug).unwrap();
        let public_key_label = confidentiality_label(web_assembly_module_signature_tag(
            &base64::decode(PUBLIC_KEY_BASE64.as_bytes())
                .expect("Couldn't decode Base64 public key"),
        ));
        let handler_command_sender = oak::io::entrypoint_node_create::<Handler, _, _>(
            "handler",
            &public_key_label,
            "app",
            LogInit {
                log_sender: Some(log_sender),
            },
        )
        .expect("Couldn't create handler node");

        Self {
            handler_command_sender,
        }
    }
}

impl oak::CommandHandler for Router {
    type Command = oak::grpc::Invocation;

    fn handle_command(&mut self, command: Self::Command) -> anyhow::Result<()> {
        forward_invocation(command, &self.handler_command_sender)
    }
}

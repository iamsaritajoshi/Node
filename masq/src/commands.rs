// Copyright (c) 2019-2020, MASQ (https://masq.ai) and/or its affiliates. All rights reserved.

use crate::command_context::{CommandContext, ContextError};
use crate::commands::CommandError::{
    ConnectionDropped, Other, Payload, Transmission, UnexpectedResponse,
};
use clap::{value_t, App, SubCommand};
use masq_lib::messages::{
    FromMessageBody, ToMessageBody, UiMessageError, UiSetupRequest, UiSetupResponse, UiSetupValue,
    UiShutdownRequest, UiShutdownResponse, UiStartOrder, UiStartResponse, NODE_NOT_RUNNING_ERROR,
};
use masq_lib::shared_schema::shared_app;
use masq_lib::ui_gateway::{NodeFromUiMessage, NodeToUiMessage};
use std::fmt::Debug;
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum CommandError {
    ConnectionRefused,
    ConnectionDropped,
    Transmission(String),
    Reception(String),
    UnexpectedResponse(UiMessageError),
    Payload(u64, String),
    Other(String),
}

pub trait Command: Debug {
    fn execute(&self, context: &mut dyn CommandContext) -> Result<(), CommandError>;
}

pub fn setup_subcommand() -> App<'static, 'static> {
    shared_app(SubCommand::with_name("setup")
        .about("Establishes (if Node is not already running) and displays startup parameters for MASQNode."))
}

#[derive(Debug, PartialEq)]
pub struct SetupCommand {
    pub values: Vec<UiSetupValue>,
}

impl Command for SetupCommand {
    fn execute(&self, context: &mut dyn CommandContext) -> Result<(), CommandError> {
        let out_message = UiSetupRequest {
            values: self.values.clone(),
        };
        let result: Result<UiSetupResponse, CommandError> = transaction(out_message, context);
        match result {
            Ok(mut response) => {
                response.values.sort_by(|a, b| {
                    a.name
                        .partial_cmp(&b.name)
                        .expect("String comparison failed")
                });
                if response.running {
                    writeln!(context.stdout(), "Note: no changes were made to the setup because the Node is currently running.")
                        .expect ("writeln! failed");
                }
                writeln!(context.stdout(), "NAME                      VALUE")
                    .expect("writeln! failed");
                response.values.into_iter().for_each(|value| {
                    writeln!(context.stdout(), "{:26}{}", value.name, value.value)
                        .expect("writeln! failed")
                });
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

impl SetupCommand {
    pub fn new(pieces: Vec<String>) -> Self {
        let matches = setup_subcommand().get_matches_from(&pieces);
        let mut values = pieces
            .iter()
            .filter(|piece| (*piece).starts_with("--"))
            .map(|piece| piece[2..].to_string())
            .map(|key| {
                let value = value_t!(matches, &key, String).expect("Value disappeared!");
                UiSetupValue::new(&key, &value)
            })
            .collect::<Vec<UiSetupValue>>();
        values.sort_by(|a, b| {
            a.name
                .partial_cmp(&b.name)
                .expect("String comparison failed")
        });
        Self { values }
    }
}

pub fn start_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("start")
        .about("Starts a MASQNode with the parameters that have been established by 'setup.' Only valid if Node is not already running.")
}

#[derive(Debug, PartialEq, Default)]
pub struct StartCommand {}

impl Command for StartCommand {
    fn execute(&self, context: &mut dyn CommandContext) -> Result<(), CommandError> {
        let out_message = UiStartOrder {};
        let result: Result<UiStartResponse, CommandError> = transaction(out_message, context);
        match result {
            Ok(response) => {
                writeln!(
                    context.stdout(),
                    "MASQNode successfully started as process {}, listening for UIs on port {}",
                    response.new_process_id,
                    response.redirect_ui_port
                )
                .expect("write! failed");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

impl StartCommand {
    pub fn new() -> Self {
        Self::default()
    }
}

const DEFAULT_SHUTDOWN_ATTEMPT_INTERVAL: u64 = 250; // milliseconds
const DEFAULT_SHUTDOWN_ATTEMPT_LIMIT: u64 = 4;

#[derive(Debug, PartialEq)]
pub struct ShutdownCommand {
    attempt_interval: u64,
    attempt_limit: u64,
}

pub fn shutdown_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("shutdown")
        .about("Shuts down the running MASQNode. Only valid if Node is already running.")
}

impl Command for ShutdownCommand {
    fn execute(&self, context: &mut dyn CommandContext) -> Result<(), CommandError> {
        let mut attempts_remaining = self.attempt_limit;
        let input = UiShutdownRequest {};
        loop {
            let output: Result<UiShutdownResponse, CommandError> =
                transaction(input.clone(), context);
            match output {
                Ok(_) => (),
                Err(ConnectionDropped) => {
                    writeln!(
                        context.stdout(),
                        "MASQNode was instructed to shut down and has broken its connection"
                    )
                    .expect("write! failed");
                    return Ok(());
                }
                Err(Transmission(msg)) => return Err(Transmission(msg)),
                Err(Payload(code, message)) if code == NODE_NOT_RUNNING_ERROR => {
                    writeln!(
                        context.stderr(),
                        "MASQNode is not running; therefore it cannot be shut down."
                    )
                    .expect("write! failed");
                    return Err(Payload(code, message));
                }
                Err(impossible) => panic!("Never happen: {:?}", impossible),
            }
            thread::sleep(Duration::from_millis(self.attempt_interval));
            attempts_remaining -= 1;
            if attempts_remaining == 0 {
                writeln!(
                    context.stderr(),
                    "MASQNode ignored the instruction to shut down and is still running"
                )
                .expect("write! failed");
                return Err(Other("Shutdown failed".to_string()));
            }
        }
    }
}

impl Default for ShutdownCommand {
    fn default() -> Self {
        Self {
            attempt_interval: DEFAULT_SHUTDOWN_ATTEMPT_INTERVAL,
            attempt_limit: DEFAULT_SHUTDOWN_ATTEMPT_LIMIT,
        }
    }
}

impl ShutdownCommand {
    pub fn new() -> Self {
        Self::default()
    }
}

fn transaction<I, O>(input: I, context: &mut dyn CommandContext) -> Result<O, CommandError>
where
    I: ToMessageBody,
    O: FromMessageBody,
{
    let ntum: NodeToUiMessage = match context.transact(NodeFromUiMessage {
        client_id: 0,
        body: input.tmb(0),
    }) {
        Ok(ntum) => ntum,
        Err(ContextError::ConnectionRefused(s)) => unimplemented!("{}", s),
        Err(ContextError::ConnectionDropped(_)) => return Err(ConnectionDropped),
        Err(ContextError::PayloadError(code, message)) => return Err(Payload(code, message)),
        Err(ContextError::RedirectFailure(e)) => panic!("Couldn't redirect to Node: {:?}", e),
        Err(ContextError::Other(msg)) => {
            writeln!(
                context.stderr(),
                "Couldn't send command to Node or Daemon: {}",
                msg
            )
            .expect("write! failed");
            return Err(Transmission(msg));
        }
    };
    let response: O = match O::fmb(ntum.body) {
        Ok((r, _)) => r,
        Err(e) => {
            writeln!(
                context.stderr(),
                "Node or Daemon is acting erratically: {}",
                e
            )
            .expect("write! failed");
            return Err(UnexpectedResponse(e));
        }
    };
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_context::ContextError;
    use crate::command_factory::{CommandFactory, CommandFactoryReal};
    use crate::commands::CommandError::{Other, Payload, Transmission, UnexpectedResponse};
    use crate::test_utils::mocks::CommandContextMock;
    use masq_lib::messages::{
        UiSetupRequest, UiSetupResponse, UiShutdownRequest, UiShutdownResponse, UiStartOrder,
        UiStartResponse, NODE_NOT_RUNNING_ERROR,
    };
    use masq_lib::ui_gateway::MessagePath::TwoWay;
    use masq_lib::ui_gateway::MessageTarget::ClientId;
    use masq_lib::ui_gateway::{MessageBody, NodeFromUiMessage, NodeToUiMessage};
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    #[test]
    fn two_way_transaction_passes_dropped_connection_error() {
        let mut context = CommandContextMock::new()
            .transact_result(Err(ContextError::ConnectionDropped("booga".to_string())));

        let result: Result<UiStartResponse, CommandError> =
            transaction(UiStartOrder {}, &mut context);

        assert_eq!(result, Err(ConnectionDropped));
    }

    #[test]
    fn two_way_transaction_passes_payload_error() {
        let mut context = CommandContextMock::new()
            .transact_result(Err(ContextError::PayloadError(10, "booga".to_string())));

        let result: Result<UiStartResponse, CommandError> =
            transaction(UiStartOrder {}, &mut context);

        assert_eq!(result, Err(Payload(10, "booga".to_string())));
    }

    #[test]
    fn two_way_transaction_passes_other_error() {
        let mut context = CommandContextMock::new()
            .transact_result(Err(ContextError::Other("booga".to_string())));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();

        let result: Result<UiStartResponse, CommandError> =
            transaction(UiStartOrder {}, &mut context);

        assert_eq!(result, Err(Transmission("booga".to_string())));
        assert_eq!(stdout_arc.lock().unwrap().get_string(), String::new());
        assert_eq!(
            stderr_arc.lock().unwrap().get_string(),
            "Couldn't send command to Node or Daemon: booga\n".to_string()
        );
    }

    #[test]
    fn two_way_transaction_handles_deserialization_error() {
        let mut context = CommandContextMock::new().transact_result(Ok(NodeToUiMessage {
            target: ClientId(0),
            body: MessageBody {
                opcode: "booga".to_string(),
                path: TwoWay(1234),
                payload: Ok("unparseable".to_string()),
            },
        }));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();

        let result: Result<UiStartResponse, CommandError> =
            transaction(UiStartOrder {}, &mut context);

        assert_eq!(
            result,
            Err(UnexpectedResponse(UiMessageError::UnexpectedMessage(
                "booga".to_string(),
                TwoWay(1234)
            )))
        );
        assert_eq!(stdout_arc.lock().unwrap().get_string(), String::new());
        assert_eq! (stderr_arc.lock().unwrap().get_string(), "Node or Daemon is acting erratically: Unexpected two-way message from context 1234 with opcode 'booga'\n".to_string());
    }

    #[test]
    fn setup_command_happy_path_with_node_not_running() {
        let transact_params_arc = Arc::new(Mutex::new(vec![]));
        let mut context = CommandContextMock::new()
            .transact_params(&transact_params_arc)
            .transact_result(Ok(NodeToUiMessage {
                target: ClientId(0),
                body: UiSetupResponse {
                    running: false,
                    values: vec![
                        UiSetupValue::new("chain", "ropsten"),
                        UiSetupValue::new("neighborhood-mode", "zero-hop"),
                    ],
                }
                .tmb(0),
            }));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();
        let factory = CommandFactoryReal::new();
        let subject = factory
            .make(vec![
                "setup".to_string(),
                "--neighborhood-mode".to_string(),
                "zero-hop".to_string(),
                "--chain".to_string(),
                "ropsten".to_string(),
            ])
            .unwrap();

        let result = subject.execute(&mut context);

        assert_eq!(result, Ok(()));
        let transact_params = transact_params_arc.lock().unwrap();
        assert_eq!(
            *transact_params,
            vec![NodeFromUiMessage {
                client_id: 0,
                body: UiSetupRequest {
                    values: vec![
                        UiSetupValue::new("chain", "ropsten"),
                        UiSetupValue::new("neighborhood-mode", "zero-hop"),
                    ]
                }
                .tmb(0)
            }]
        );
        assert_eq! (stdout_arc.lock().unwrap().get_string(),
            "NAME                      VALUE\nchain                     ropsten\nneighborhood-mode         zero-hop\n");
        assert_eq!(stderr_arc.lock().unwrap().get_string(), String::new());
    }

    #[test]
    fn setup_command_happy_path_with_node_running() {
        let transact_params_arc = Arc::new(Mutex::new(vec![]));
        let mut context = CommandContextMock::new()
            .transact_params(&transact_params_arc)
            .transact_result(Ok(NodeToUiMessage {
                target: ClientId(0),
                body: UiSetupResponse {
                    running: true,
                    values: vec![
                        UiSetupValue::new("chain", "ropsten"),
                        UiSetupValue::new("neighborhood-mode", "zero-hop"),
                    ],
                }
                .tmb(0),
            }));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();
        let factory = CommandFactoryReal::new();
        let subject = factory
            .make(vec![
                "setup".to_string(),
                "--neighborhood-mode".to_string(),
                "zero-hop".to_string(),
                "--chain".to_string(),
                "ropsten".to_string(),
            ])
            .unwrap();

        let result = subject.execute(&mut context);

        assert_eq!(result, Ok(()));
        let transact_params = transact_params_arc.lock().unwrap();
        assert_eq!(
            *transact_params,
            vec![NodeFromUiMessage {
                client_id: 0,
                body: UiSetupRequest {
                    values: vec![
                        UiSetupValue::new("chain", "ropsten"),
                        UiSetupValue::new("neighborhood-mode", "zero-hop"),
                    ]
                }
                .tmb(0)
            }]
        );
        assert_eq! (stdout_arc.lock().unwrap().get_string(),
            "Note: no changes were made to the setup because the Node is currently running.\nNAME                      VALUE\nchain                     ropsten\nneighborhood-mode         zero-hop\n");
        assert_eq!(stderr_arc.lock().unwrap().get_string(), String::new());
    }

    #[test]
    fn start_command_happy_path() {
        let transact_params_arc = Arc::new(Mutex::new(vec![]));
        let mut context = CommandContextMock::new()
            .transact_params(&transact_params_arc)
            .transact_result(Ok(NodeToUiMessage {
                target: ClientId(0),
                body: UiStartResponse {
                    new_process_id: 1234,
                    redirect_ui_port: 4321,
                }
                .tmb(0),
            }));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();
        let factory = CommandFactoryReal::new();
        let subject = factory.make(vec!["start".to_string()]).unwrap();

        let result = subject.execute(&mut context);

        assert_eq!(result, Ok(()));
        let transact_params = transact_params_arc.lock().unwrap();
        assert_eq!(
            *transact_params,
            vec![NodeFromUiMessage {
                client_id: 0,
                body: UiStartOrder {}.tmb(0)
            }]
        );
        assert_eq!(
            stdout_arc.lock().unwrap().get_string(),
            "MASQNode successfully started as process 1234, listening for UIs on port 4321\n"
        );
        assert_eq!(stderr_arc.lock().unwrap().get_string(), String::new());
    }

    #[test]
    fn shutdown_command_defaults_parameters() {
        let subject = ShutdownCommand::new();

        assert_eq!(subject.attempt_interval, DEFAULT_SHUTDOWN_ATTEMPT_INTERVAL);
        assert_eq!(subject.attempt_limit, DEFAULT_SHUTDOWN_ATTEMPT_LIMIT);
    }

    #[test]
    fn testing_command_factory_here() {
        let factory = CommandFactoryReal::new();
        let mut context = CommandContextMock::new()
            .transact_result(Err(ContextError::ConnectionDropped("booga".to_string())));
        let subject = factory.make(vec!["shutdown".to_string()]).unwrap();

        let result = subject.execute(&mut context);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn shutdown_command_doesnt_work_if_node_is_not_running() {
        let mut context = CommandContextMock::new().transact_result(Err(
            ContextError::PayloadError(NODE_NOT_RUNNING_ERROR, "irrelevant".to_string()),
        ));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();
        let subject = ShutdownCommand::new();

        let result = subject.execute(&mut context);

        assert_eq!(
            result,
            Err(CommandError::Payload(
                NODE_NOT_RUNNING_ERROR,
                "irrelevant".to_string()
            ))
        );
        assert_eq!(
            stderr_arc.lock().unwrap().get_string(),
            "MASQNode is not running; therefore it cannot be shut down.\n"
        );
        assert_eq!(stdout_arc.lock().unwrap().get_string(), String::new());
    }

    #[test]
    fn shutdown_command_happy_path() {
        let transact_params_arc = Arc::new(Mutex::new(vec![]));
        let msg = NodeToUiMessage {
            target: ClientId(0),
            body: UiShutdownResponse {}.tmb(0),
        };
        let mut context = CommandContextMock::new()
            .transact_params(&transact_params_arc)
            .transact_result(Ok(msg.clone()))
            .transact_result(Ok(msg.clone()))
            .transact_result(Err(ContextError::ConnectionDropped("booga".to_string())));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();
        let mut subject = ShutdownCommand::new();
        subject.attempt_interval = 10;
        subject.attempt_limit = 3;

        let result = subject.execute(&mut context);

        assert_eq!(result, Ok(()));
        let transact_params = transact_params_arc.lock().unwrap();
        assert_eq!(
            *transact_params,
            vec![
                NodeFromUiMessage {
                    client_id: 0,
                    body: UiShutdownRequest {}.tmb(0)
                },
                NodeFromUiMessage {
                    client_id: 0,
                    body: UiShutdownRequest {}.tmb(0)
                },
                NodeFromUiMessage {
                    client_id: 0,
                    body: UiShutdownRequest {}.tmb(0)
                },
            ]
        );
        assert_eq!(
            stdout_arc.lock().unwrap().get_string(),
            "MASQNode was instructed to shut down and has broken its connection\n"
        );
        assert_eq!(stderr_arc.lock().unwrap().get_string(), String::new());
    }

    #[test]
    fn shutdown_command_uses_interval() {
        let mut context = CommandContextMock::new().transact_result(Ok(NodeToUiMessage {
            target: ClientId(0),
            body: UiShutdownResponse {}.tmb(0),
        }));
        let stdout_arc = context.stdout_arc();
        let stderr_arc = context.stderr_arc();
        let mut subject = ShutdownCommand::new();
        subject.attempt_interval = 100;
        subject.attempt_limit = 1;
        let before = SystemTime::now();

        let result = subject.execute(&mut context);

        let after = SystemTime::now();
        assert_eq!(result, Err(Other("Shutdown failed".to_string())));
        let interval = after.duration_since(before).unwrap().as_millis();
        assert!(
            interval >= subject.attempt_interval as u128,
            "Not waiting long enough per attempt: {} < {}",
            interval,
            subject.attempt_interval
        );
        assert!(
            interval < (subject.attempt_interval as u128 * 5),
            "Waiting too long per attempt: {} >> {}",
            interval,
            subject.attempt_interval
        );
        assert_eq!(stdout_arc.lock().unwrap().get_string(), String::new());
        assert_eq!(
            stderr_arc.lock().unwrap().get_string(),
            "MASQNode ignored the instruction to shut down and is still running\n"
        );
    }
}

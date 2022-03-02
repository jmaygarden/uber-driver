use crate::UberServerError;
use mlua::{FromLua, ToLua, ToLuaMulti};
use std::{process::Output, rc::Rc, time::Duration};
use tokio::process::Command;
use uber_protos::DriverResponse;

const REGISTRY_COROUTINES: &str = "REGISTRY_COROUTINES";
const REGISTRY_SANDBOX: &str = "REGISTRY_SANDBOX";

pub struct Executor {
    lua: Rc<mlua::Lua>,
}

impl Executor {
    pub fn new() -> Result<Self, UberServerError> {
        let lua = Rc::new(unsafe { mlua::Lua::unsafe_new() });

        let table = lua.create_table()?;
        lua.set_named_registry_value(REGISTRY_COROUTINES, table)?;

        let table = lua
            .load(mlua::chunk! {
                local REQUEST_NOOP = 0
                local REQUEST_PRINT = 1
                local REQUEST_SLEEP = 2
                local REQUEST_GETDATE = 3

                function noop()
                    coroutine.yield(REQUEST_NOOP)
                end

                function print(...)
                    local msg = table.concat({...}, "\t")

                    coroutine.yield(REQUEST_PRINT, msg)
                end

                function sleep(duration)
                    coroutine.yield(REQUEST_SLEEP, duration)
                end

                function get_date()
                    return coroutine.yield(REQUEST_GETDATE)
                end

                return { __index = _G }
            })
            .eval::<mlua::Table>()?;
        lua.set_named_registry_value(REGISTRY_SANDBOX, table)?;

        Ok(Self { lua })
    }

    pub fn create_coroutine(
        &mut self,
        driver_id: String,
        bytecode: Vec<u8>,
    ) -> Result<(), UberServerError> {
        store_thread(self.lua.clone(), driver_id.as_str(), &bytecode)?;

        tokio::task::spawn_local(spawn_thread(self.lua.clone(), driver_id));

        Ok(())
    }

    pub fn kill_coroutine(&mut self, driver_id: String) -> DriverResponse {
        let lua = self.lua.clone();
        let result = load_thread(&self.lua, driver_id.as_str()).and_then(|thread| {
            let function = lua.create_function(|_, _: ()| Ok(()))?;

            thread.reset(function).map_err(UberServerError::LuaError)
        });
        let error = match result {
            Ok(()) => None,
            Err(error) => Some(error.to_string()),
        };

        DriverResponse { driver_id, error }
    }
}

fn store_thread(
    lua: Rc<mlua::Lua>,
    driver_id: &str,
    bytecode: &Vec<u8>,
) -> Result<(), UberServerError> {
    let env = lua.create_table()?;
    let metatable = lua.named_registry_value::<_, mlua::Table>(REGISTRY_SANDBOX)?;
    env.set_metatable(Some(metatable));

    let chunk = lua.load(&bytecode);
    let function = chunk
        .set_name(driver_id)?
        .set_environment(env)?
        .into_function()?;
    let thread = lua.create_thread(function)?;
    let registry: mlua::Table = lua.named_registry_value(REGISTRY_COROUTINES)?;

    registry
        .set(driver_id, thread)
        .map_err(UberServerError::LuaError)
}

fn load_thread<'lua>(
    lua: &'lua mlua::Lua,
    driver_id: &str,
) -> Result<mlua::Thread<'lua>, UberServerError> {
    let registry = lua.named_registry_value::<_, mlua::Table>(REGISTRY_COROUTINES)?;

    mlua::Thread::from_lua(registry.get(driver_id)?, lua).map_err(UberServerError::LuaError)
}

async fn spawn_thread(lua: Rc<mlua::Lua>, driver_id: String) {
    let thread = match load_thread(&lua, driver_id.as_str()) {
        Ok(thread) => thread,
        Err(error) => {
            log::error!("{driver_id}: {error}");
            return;
        }
    };

    let nil = mlua::MultiValue::new();
    let mut args = Some(nil.clone());

    while let mlua::ThreadStatus::Resumable = thread.status() {
        match thread.resume::<_, AsyncRequest>(args.take().unwrap_or(nil.clone())) {
            Ok(request) => {
                log::info!("{driver_id}: {request:?}");

                match request {
                    AsyncRequest::NoOp => tokio::task::yield_now().await,
                    AsyncRequest::Print(msg) => {
                        log::info!("{driver_id}: {msg}");
                        tokio::task::yield_now().await;
                    }
                    AsyncRequest::Sleep(duration) => tokio::time::sleep(duration).await,
                    AsyncRequest::GetDate => {
                        let result = Command::new("date")
                            .output()
                            .await
                            .map_err(UberServerError::IoError)
                            .and_then(
                                |Output {
                                     status,
                                     stdout,
                                     stderr,
                                 }| {
                                    let table = lua.create_table()?;
                                    table.set("status", status.to_string())?;
                                    table.set("stdout", String::from_utf8(stdout)?)?;
                                    table.set("stderr", String::from_utf8(stderr)?)?;

                                    let value = table.to_lua_multi(&lua)?;

                                    Ok(value)
                                },
                            )
                            .map_err(|err| {
                                let values = vec![
                                    mlua::Value::Nil,
                                    err.to_string()
                                        .to_lua(&lua)
                                        .unwrap_or_else(|error| mlua::Value::Error(error)),
                                ];

                                mlua::MultiValue::from_vec(values)
                            });

                        args.replace(match result {
                            Ok(value) => value,
                            Err(value) => value,
                        });
                    }
                }
            }
            Err(error) => {
                if let mlua::ThreadStatus::Resumable = thread.status() {
                    log::error!("{driver_id}: {error}");
                } else {
                    log::info!("{driver_id}: TERMINATED");
                }
            }
        }
    }
}

#[derive(Debug)]
enum AsyncRequest {
    NoOp,
    Print(String),
    Sleep(Duration),
    GetDate,
}

impl<'lua> mlua::FromLuaMulti<'lua> for AsyncRequest {
    fn from_lua_multi(values: mlua::MultiValue<'lua>, lua: &'lua mlua::Lua) -> mlua::Result<Self> {
        let mut values = values.into_iter();
        let opcode = match values.next() {
            Some(value) => i32::from_lua(value, lua)?,
            None => return Err(mlua::Error::RuntimeError("missing opcode".to_string())),
        };

        match opcode {
            0 => Ok(AsyncRequest::NoOp),
            1 => {
                let msg = match values.next() {
                    Some(value) => String::from_lua(value, lua)?,
                    None => String::default(),
                };

                Ok(AsyncRequest::Print(msg))
            }
            2 => {
                let secs = match values.next() {
                    Some(value) => f64::from_lua(value, lua)?,
                    None => 0f64,
                };

                Ok(AsyncRequest::Sleep(Duration::from_secs_f64(secs)))
            }
            3 => Ok(AsyncRequest::GetDate),
            _ => {
                return Err(mlua::Error::RuntimeError(format!(
                    "invalid opcode: {opcode}"
                )))
            }
        }
    }
}

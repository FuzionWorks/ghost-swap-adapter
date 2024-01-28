#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, wasm_execute, BankMsg, Binary, Deps, DepsMut, Empty, Env, MessageInfo,
    Response, StdResult,
};
use cw2::set_contract_version;
use cw_utils::one_coin;
use kujira::{bow, ghost, KujiraMsg, KujiraQuery};

use crate::msg::Config;
use crate::state::CONFIG;
use crate::{ContractError, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};

const CONTRACT_NAME: &str = "entropic/swap-adapter";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn migrate(deps: DepsMut<KujiraQuery>, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[entry_point]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config { owner: msg.owner };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    match msg {
        ExecuteMsg::UpdateConfig { owner } => {
            ensure!(info.sender == config.owner, ContractError::Unauthorized {});
            if let Some(owner) = owner {
                config.owner = owner;
            }
            CONFIG.save(deps.storage, &config)?;
            Ok(Response::default())
        }
        ExecuteMsg::Swap { callback, .. } => {
            let received = one_coin(&info)?;

            ensure!(
                received.denom.starts_with("factory/"),
                ContractError::InvalidDenom(received.denom)
            );

            let split = received.denom.split('/').collect::<Vec<&str>>();
            ensure!(
                split.len() == 3,
                ContractError::InvalidDenom(received.denom)
            );
            let addr = split[1];
            let subdenom = split[2];
            let msg = match subdenom {
                "ulp" => {
                    let msg = bow::market_maker::ExecuteMsg::Withdraw { callback: None };
                    wasm_execute(addr, &msg, info.funds)?
                }
                "urcpt" => {
                    let msg = ghost::receipt_vault::ExecuteMsg::Withdraw(
                        ghost::receipt_vault::WithdrawMsg { callback: None },
                    );
                    wasm_execute(addr, &msg, info.funds)?
                }
                _ => return Err(ContractError::InvalidDenom(received.denom)),
            };

            let post_swap_msg = wasm_execute(
                env.contract.address,
                &ExecuteMsg::PostSwap {
                    callback,
                    sender: info.sender,
                },
                vec![],
            )?;
            Ok(Response::new().add_message(msg).add_message(post_swap_msg))
        }
        ExecuteMsg::PostSwap { callback, sender } => {
            ensure!(
                info.sender == env.contract.address,
                ContractError::Unauthorized {}
            );
            let funds = deps.querier.query_all_balances(env.contract.address)?;

            let return_msg = match callback {
                Some(callback) => callback.to_message(&sender, Empty {}, funds)?,
                None => BankMsg::Send {
                    to_address: sender.to_string(),
                    amount: funds,
                }
                .into(),
            };

            Ok(Response::new().add_message(return_msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, _: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    Ok(match msg {
        QueryMsg::Config {} => to_json_binary(&config),
    }?)
}

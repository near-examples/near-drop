use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{Account, Contract, DevNetwork, Worker};

pub async fn init(
    worker: &Worker<impl DevNetwork>,
    root: &Account,
) -> anyhow::Result<(Contract, Account, Account)> {
    let wasm = near_workspaces::compile_project(".").await?;
    let contract = worker.dev_deploy(&wasm).await?;

    let creator = root
        .create_subaccount("creator")
        .initial_balance(NearToken::from_near(2))
        .transact()
        .await?
        .unwrap();
    let alice = root.create_subaccount("alice").transact().await?.unwrap();

    let res = contract
        .call("new")
        .args_json(json!({"top_level_account": root.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok((contract, creator, alice));
}

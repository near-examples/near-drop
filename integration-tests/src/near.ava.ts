import { Worker, NearAccount, NEAR } from 'near-workspaces';
import anyTest, { TestFn } from 'ava';
import { BN } from 'bn.js';
import { read, readFileSync } from 'fs';

const test = anyTest as TestFn<{
  worker: Worker;
  accounts: Record<string, NearAccount>;
}>;

const TGAS = new BN("1000000000000")

test.beforeEach(async (t) => {
  // Init the worker and start a Sandbox server
  const worker = await Worker.init();
  const root = worker.rootAccount;

  // Deploy TDA contract
  await root.deploy('./aux/TLA.wasm')

  // Deploy contract
  const contract = await root.createSubAccount('contract');
  await contract.deploy(process.argv[2]);

  // Initialize the contract
  await contract.call(contract, "new", { top_level_account: root.accountId })

  // Create test accounts
  const creator = await root.createSubAccount('creator');
  const alice = await root.createSubAccount('alice');

  // Save state for test runs, it is unique for each test
  t.context.worker = worker;
  t.context.accounts = { root, contract, creator, alice };
});

test.afterEach.always(async (t) => {
  // Stop Sandbox server
  await t.context.worker.tearDown().catch((error) => {
    console.log('Failed to stop the Sandbox:', error);
  });
});

test('drop on an existing account', async (t) => {
  const { contract, creator, alice } = t.context.accounts;

  // Check the account balances
  const contractBalance = await contract.availableBalance()
  const aliceBalance = await alice.availableBalance()

  // Create a public key and add as a drop key 
  const pkDrop = await alice.getKey()!;

  await creator.call(
    contract, "create_near_drop",
    {
      public_key: pkDrop?.getPublicKey()?.toString(),
      tokens: NEAR.parse("1 N").toString()
    },
    { attachedDeposit: "1045400000000000000000000" } // 1.0264 N
  )

  // Use the key to call "claim_for" to claim NEAR for account_id 
  await contract.setKey(pkDrop!)
  const claim = await contract.call(contract, "claim_for", { account_id: alice.accountId }, { gas: TGAS.muln(85), signWithKey: pkDrop! })
  t.true(claim)

  // The new balance should be exactly one near more
  const aliceNewBalance = await alice.availableBalance()
  t.deepEqual(aliceNewBalance, aliceBalance.add(NEAR.parse("1 N")))

  // Try to use the key again
  const callAgain = contract.call(contract, "claim_for", { account_id: alice.accountId }, { gas: TGAS.muln(85), signWithKey: pkDrop! })
  await t.throwsAsync(callAgain)

  // Ideally, there should be no surplus in the contract
  const contractNewBalance = await contract.availableBalance()
  t.true(contractNewBalance >= contractBalance)

  console.log("EA - Contract balance surplus:", contractNewBalance.sub(contractBalance).toHuman())
});

test('drop on a new account', async (t) => {
  const { contract, creator, alice } = t.context.accounts;

  // Get the contract's balance
  const contractBalance = await contract.availableBalance()

  // Create a key and add it to allow claiming a
  const pkDrop = await alice.getKey();

  await creator.call(
    contract, "create_near_drop",
    {
      tokens: NEAR.parse("1 N").toString(),
      public_key: pkDrop?.getPublicKey()?.toString()
    },
    { attachedDeposit: "1045400000000000000000000" }
  )

  // Try to claim on the longest account string
  const longId = "a12345678901234567890123456789012345678901234567890123.test.near"

  await contract.setKey(pkDrop!)
  const claim = await contract.call(contract, "create_account_and_claim", { account_id: longId }, { gas: TGAS.muln(100), signWithKey: pkDrop! })
  t.true(claim)

  // Check the balance of the new account
  const long = contract.getAccount(longId)
  const longBalance = await long.availableBalance()
  console.log("Long Account Balance:", longBalance.toHuman())

  // Try to call again and check it fails
  const callAgain = contract.call(contract, "create_account_and_claim", { account_id: longId }, { gas: TGAS.muln(100), signWithKey: pkDrop! })
  await t.throwsAsync(callAgain)

  // Ideally there should be no surplus in the contract
  const contractNewBalance = await contract.availableBalance()
  t.true(contractNewBalance >= contractBalance)

  console.log("NA - Contract surplus:", contractNewBalance.sub(contractBalance).toHuman())
});

test('drop on a new account with no money', async (t) => {
  const { contract, creator, alice } = t.context.accounts;

  // Get the contract's balance
  const contractBalance = await contract.availableBalance()

  // Create an account and add it as a 1yN drop
  const pkDrop = await alice.getKey();

  await creator.call(
    contract, "create_near_drop",
    {
      tokens: "1",
      public_key: pkDrop?.getPublicKey()?.toString()
    },
    { attachedDeposit: "45400000000000000000001" }
  )

  // Create an account with the longest possible name
  await contract.setKey(pkDrop!)
  const longId = "a12345678901234567890123456789012345678901234567890123.test.near"
  const claim = await contract.call(contract, "create_account_and_claim", { account_id: longId }, { gas: TGAS.muln(100), signWithKey: pkDrop! })
  t.true(claim)

  // Check the new account's balance
  const long = contract.getAccount(longId)
  const longBalance = await long.availableBalance()
  console.log("1yN Account balance:", longBalance.toHuman())

  // Try to call again and it should fail
  const callAgain = contract.call(contract, "create_account_and_claim", { account_id: longId }, { gas: TGAS.muln(100), signWithKey: pkDrop! })
  await t.throwsAsync(callAgain)

  // Check the contract's surplus
  const contractNewBalance = await contract.availableBalance()
  t.true(contractNewBalance >= contractBalance)

  console.log("1yN - Contract's surplus", contractNewBalance.sub(contractBalance).toHuman())
});
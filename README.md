# move-to-ts
From your package directory, run these:
```
aptos move compile          # need this to fetch dependencies
move-to-ts                  # this creates a typescript folder under build, with all the transpiled ts files
```

A Move-to-TypeScript transpiler & emulator. Everything *seems* to work now. We have a testsuite that aggregates 347 unit
tests from these Move packages:
- [AptosFramework](https://github.com/aptos-labs/aptos-core/tree/main/aptos-move/framework)
- [HippoSwap](https://github.com/hippospace/hippo-swap)
- [TokenRegistry](https://github.com/hippospace/aptos-registry)
- [Econia](https://github.com/econia-labs/econia)

We are only failing 6 out of 347 test cases. All 6 failures are due to unimplemented hashing and signature-verification
native functions. We are continuously adding more tests and packages to the testsuite so as to stress test our
transpiler.
```
Tests:       6 failed, 347 passed, 353 total
```

To run the testsuite:
```
cd move-testsuite
aptos move compile                                      # fetch dependencies
move-to-ts --test --pakcage-json-name myPackageName     # build tests and the full ts package
cd build/typescript
yarn install                                            # install dependencies
yarn build
yarn test                                               # run all unit tests with jest
```


# Why?

Automatically translate Move code to TypeScript. So that:
- swap frontends and arb bots can generate quotes using exactly the same pricing logic as its Move contract
- liquidation bots can evaluate whether an account is open for liquidation using exactly the same logic as contract

Above are two very specific use cases. In a more abstract sense, `move-to-ts` wants to increase the portability of 
Move code and increase the productivity of Move developers.

A side benefit of having an emulator, of course, is that we can also debug Move code in a JS environment (since we 
are currently missing a native Move debugger).


# Long-term objectives

Turn this into a development framework similar to Truffle/Anchor, and provide direct support for
- frontend wallet integration
- off-chain data integration
- automated TypeScript SDK generation
- automated bot script generation
- automated experimental UI generation
  

# Experimental objectives:

- Generate programmer-guided, informal proof-of-correctness


# Example
Here's the `AccountUtil.move` from AptosFramework:
```
module AptosFramework::AccountUtils {
    use AptosFramework::Account;
    use AptosFramework::Coin;
    use AptosFramework::TestCoin::TestCoin;

    public(script) fun create_and_fund_account(funder: &signer, account: address, amount: u64) {
        Account::create_account(account);
        Coin::transfer<TestCoin>(funder, account, amount);
    }
}
```

Here's the transpiled code:
```
import * as $ from "@manahippo/move-to-ts";
import {AptosDataCache, AptosParserRepo} from "@manahippo/move-to-ts";
import {U8, U64, U128} from "@manahippo/move-to-ts";
import {TypeParamDeclType, FieldDeclType} from "@manahippo/move-to-ts";
import {AtomicTypeTag, StructTag, TypeTag, VectorTag} from "@manahippo/move-to-ts";
import {HexString, AptosClient} from "aptos";
import * as Std from "../Std";
import * as Account from "./Account";
import * as Coin from "./Coin";
export const package = "AptosFramework";
export const moduleAddress = "0x1";
export const moduleName = "AccountUtils";



export function create_and_fund_account (
  funder: HexString,
  account: HexString,
  amount: U64,
  $c: $.AptosDataCache,
): void {
  Account.create_account(account, $c);
  Coin.transfer(funder, account, amount, $c, [new StructTag(new HexString("0x1"), "TestCoin", "TestCoin", [])]);
  return;
}


export function buildPayload_create_and_fund_account (
  funder: HexString,
  account: HexString,
  amount: U64,
) {
  const typeParamStrings = "";
  return $.buildPayload(
    "0x1::AccountUtils::create_and_fund_account",
    typeParamStrings,
    [
      account,
      amount.toPayloadArg(),
    ]
  );

}

```


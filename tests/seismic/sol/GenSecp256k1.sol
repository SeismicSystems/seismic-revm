// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract GENSECP256K1 {
    function genKey(bytes32 rngPers) public view returns (bytes memory) {
        address gen_precompile = address(0x65);

        bytes memory input = bytes.concat(rngPers);

        // Call the precompile
        (bool success, bytes memory output) = gen_precompile.staticcall(input);
        // Ensure the call was successful
        require(success, "GenSecp256k1KeysPrecompile call failed");

        assembly {
            let len := mload(output)
            let data := add(output, 32)
            return(data, len)
        }
    }
}
// TODO: fix the test case. won't accept long expected values
// ====
// EVMVersion: >=mercury
// ====
// ----
// genKey(bytes32): 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef ->  0x7045d680ba85139d5338eca1d5aa9b3f1da3992024b466cd0afb2fedce3188bb

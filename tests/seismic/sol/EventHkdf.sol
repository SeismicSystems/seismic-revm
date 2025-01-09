// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract HKDF_DERIVE_AES_KEY {

    uint256 eventCtr = 0;
    uint256 ssalt; // TODO: make this an suint

    constructor() {
        // TODO: replace with something from the rng precompile
        ssalt = uint256(keccak256(abi.encodePacked("salt"))); 
    }


    /// @notice Derives an AES key to Encrypt an Event using a precompiled contract.
    /// @return result The derived AES key as bytes32.
    function deriveAESKey() public returns (bytes32 result) {
        // Address of the precompiled contract
        address HkdfPrecompile = address(0x69);

        // Concatenate msg.sender, eventCtr for uniqueness,
        // blockhash for additional entropy, and ssalt as a secret salt
        bytes memory input = abi.encodePacked(msg.sender, eventCtr, blockhash(block.number), ssalt);

        // Call the precompiled contract
        (bool success, bytes memory output) = HkdfPrecompile.staticcall(input);

        // Ensure the call was successful
        require(success, "Precompile call failed");

        // Decode the result
        require(output.length == 32, "Invalid output length");
        assembly {
            result := mload(add(output, 32))
        }

        // increment the event counter
        eventCtr += 1;
    }

    function testDeriveAESKey() public returns (bytes32 result) {
        // Create inputs by concatenating secret keys and public keys
        bytes32 result_a = deriveAESKey();
        return result_a;
    }
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testDeriveAESKey() -> hex"ecf6c4377de84f1112ebb0b87221a051ae7711145d1280c0e7f6e9fac02c3f52"
// testDeriveAESKey() -> hex"00ba90bdfc1d8507e6460649b5a3457be5b27fbec4557ca958fe37caec7f74e9"

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract GENSECP256K1 {
    /// @notice Generates a Secp256k1 key pair using the provided RNG personalization.
    /// @param rngPers The RNG personalization bytes to include caller entropy
    /// returns the generated Secp256k1 key pair, with the first 32 bytes being the secret key 
    ///         and the last 32 bytes being the public key.
    function genKeypair(bytes32 rngPers) public view returns (bytes memory) {
        address gen_precompile = address(0x65);

        bytes memory input = bytes.concat(rngPers);

        (bool success, bytes memory output) = gen_precompile.staticcall(input);
        require(success, "GenSecp256k1KeysPrecompile call failed");

        return output;
    }
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// genKeypair(bytes32): 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef ->  hex"0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000004168516fce735ec91f81ac06ef7f25f8b4c943a8174625d234f6745fd1e31894c9030db7061f2ec6bbf0af89e21cc2eeec8ae54633045422cfb36244334a3ea659a800000000000000000000000000000000000000000000000000000000000000"

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract GENSECP256K1 {
    /// @notice Generates a Secp256k1 key pair using the provided RNG personalization.
    /// @param rngPers The RNG personalization bytes to include caller entropy
    /// returns the generated Secp256k1 key pair, with the first 32 bytes being the secret key 
    ///         and the last 32 bytes being the public key.
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
// ====
// EVMVersion: >=mercury
// ====
// ----
// genKey(bytes32): 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef ->  hex"7045d680ba85139d5338eca1d5aa9b3f1da3992024b466cd0afb2fedce3188bb0343208ff9e89730e560db0f80170a7f05129ca7fc8ac84be3fd95c7104301de75"

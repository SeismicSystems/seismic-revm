// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract EMITENCEVENT {
    
    uint256 nonce = 0;

    event EncryptedEvent(bytes recipient_pk, bytes ciphertext);

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
    
    /// @notice Derives an AES key using a precompiled contract.
    /// @param sk The secret key as 32 bytes.
    /// @param pk The public key as 33 bytes.
    /// @return result The derived AES key as bytes32.
    function deriveAESKey(bytes32 sk, bytes memory pk) public view returns (bytes32 result) {
        // Address of the precompiled contract
        address deriveAESKeyPrecompile = address(0x66);

        // Concatenate secret key and public key
        bytes memory input = abi.encodePacked(sk, pk);

        // Call the precompiled contract
        (bool success, bytes memory output) = deriveAESKeyPrecompile.staticcall(input);

        // Ensure the call was successful
        require(success, "Precompile call failed");

        // Decode the result
        require(output.length == 32, "Invalid output length");
        assembly {
            result := mload(add(output, 32))
        }
    }
    
    /// @notice Endcrypts the plaintext using AES-256 GCM with the provided key and nonce.
    /// @param aes_key The 32 bit AES-256 GCM key used for encryption.
    /// @param plaintext The bytes to encrypt.
    /// @param nonce the u64 nonce for encryption, encoded as a big-endian bytes32.
    /// returns the encrypted bytes.
    function AESEncrypt(bytes32 aes_key, uint256 nonce, bytes memory plaintext) public view returns (bytes memory) {
        // Address of the precompiled contract
        address AESEncryptAddr = address(0x67);

        // Concatenate secret key and public key
        bytes memory input = abi.encodePacked(aes_key, nonce, plaintext);

        // Call the precompiled contract
        (bool success, bytes memory output) = AESEncryptAddr.staticcall(input);

        // Ensure the call was successful
        require(success, "Precompile call failed");

        return output;
    }

    function EmitEncEvent(bytes32 rngPers, bytes memory plaintext, bytes memory recipient_pk) public {
        bytes memory eph_keypair = genKeypair(rngPers);
        require(eph_keypair.length == 65, "eph_keypair too short");

        bytes32 eph_sk;
        bytes memory eph_pk = new bytes(33);

        assembly {
            eph_sk := mload(add(eph_keypair, 32)) // Load the first 32 bytes as eph_sk
        }

        for (uint256 i = 0; i < 33; i++) {
            eph_pk[i] = eph_keypair[32 + i]; // Copy the next 33 bytes for eph_pk
        }

        bytes32 aes_key = deriveAESKey(eph_sk, recipient_pk);
        nonce = nonce + 1;
        bytes memory ciphertext = AESEncrypt(aes_key, nonce, plaintext);
        emit EncryptedEvent(recipient_pk, ciphertext);
    }
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// TODO: add tests

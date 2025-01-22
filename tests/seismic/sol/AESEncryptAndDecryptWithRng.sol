// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AES {
    bytes public constant PLAINTEXT = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";

    suint96 NONCE;
    suint256 AES_KEY;
    bytes public encryptedData;

    /// @notice Generates a random nonce using seismicRng.
    function updateNonce() public {
        bytes32 rngOutput = seismicRng();
        NONCE = suint96(suint256(rngOutput)); 
    }

    /// @param _aes_key The AES key to use for encryption, in suint format.
    function set_aes_key(suint256 _aes_key) public {
        AES_KEY = _aes_key;
    }

    /// @notice Encrypts the hardcoded plaintext and stores the ciphertext in storage.
    function encryptAndStore() public {
        address AESEncryptAddr = address(0x67);

        bytes memory input = abi.encodePacked(AES_KEY, NONCE, PLAINTEXT);

        (bool success, bytes memory output) = AESEncryptAddr.staticcall(input);
        require(success, "Precompile encryption call failed");
        require(output.length > 0, "Encryption call returned no output");

        encryptedData = output;
    }

    /// @notice Decrypts the previously stored ciphertext and verifies it matches the original plaintext.
    function decryptAndVerify() public {
        require(encryptedData.length > 0, "No encrypted data available");

        address AESDecryptAddr = address(0x68);

        bytes memory input = abi.encodePacked(AES_KEY, NONCE, encryptedData);

        (bool success, bytes memory output) = AESDecryptAddr.staticcall(input);
        require(success, "Precompile decryption call failed");

        require(keccak256(output) == keccak256(PLAINTEXT), "Decrypted data does not match plaintext");
    }

    function seismicRng() public view returns (bytes32) {
         address rngPrecompile = address(0x64);

         bytes memory input = bytes.concat(bytes1(0x00));

         // Call the precompile
         (bool success, bytes memory output) = rngPrecompile.staticcall(input);
         require(success, "RNG Precompile call failed");

         bytes32 output32;
         assembly {
             output32 := mload(add(output, 32))
         }

         return output32;
     }

    function testEndToEnd(suint256 aes_key) public {
        updateNonce();
        set_aes_key(aes_key);
        encryptAndStore();
        decryptAndVerify();
    }
    
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testEndToEnd(suint256): hex"7e34abdcd62eade2e803e0a8123a0015ce542b380537eff288d6da420bcc2d3b"

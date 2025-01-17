
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AESENCRYPT {
    /// @notice Endcrypts the plaintext using AES-256 GCM with the provided key and nonce.
    /// @param aes_key The 32 bit AES-256 GCM key used for encryption.
    /// @param plaintext The bytes to encrypt.
    /// @param nonce the u64 nonce for encryption, encoded as a big-endian bytes32.
    /// returns the encrypted bytes.
    function AESEncrypt(bytes32 aes_key, uint96 nonce, bytes memory plaintext) public view returns (bytes memory) {
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

    function testAESEncrypt() public view returns (bytes memory result) {
        uint96 nonce = 17;
        bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        bytes memory plaintext = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
        result = AESEncrypt(aes_key, nonce, plaintext);
    }   
    
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testAESEncrypt() -> hex"00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000053cbf232bfd91fe00594508e3615f2f8a51f3de3d5965837cd25b550b38be2a8d5b356328a0cf91e892d0177d3a3c2ceebe5334a1655353f07d5ca1c98b442e35e39b3c86327fce9a76fe77a331bfcb510aed78200000000000000000000000000"

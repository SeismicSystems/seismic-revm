
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AESENCRYPT {
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

    function testAESEncrypt() public view returns (bytes memory result) {
        uint256 nonce = 17;
        bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        bytes memory plaintext = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
        result = AESEncrypt(aes_key, nonce, plaintext);
    }   
    

    // TODO: remove this
    // function testAESEncrypt2() public view returns (bytes32 result) {
    //     uint256 nonce = 17;
    //     bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    //     bytes memory plaintext = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
    //     bytes memory real_result = AESEncrypt(aes_key, nonce, plaintext);
    //     assembly {
    //         result := mload(add(real_result, 32))
    //     }
    //     return result;
    // }   
    // // testAESEncrypt2() -> hex"0000000000000011f577b2b34b7dbafad6647accfaa9194d7a39c839e618fdbe9fc304691385c6fdcb1a8bf1c84560871726c31334884d85b463b0d9930c50370b9cdcc492dfcfb232dd38f0b0beb1c75e6f5c07e3a9ad"

}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testAESEncrypt() -> hex"0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000004ff577b2b34b7dbafad6647accfaa9194d7a39c839e618fdbe9fc304691385c6fdcb1a8bf1c84560871726c31334884d85b463b0d9930c50370b9cdcc492dfcfb232dd38f0b0beb1c75e6f5c07e3a9ad0000000000000000000000000000000000"

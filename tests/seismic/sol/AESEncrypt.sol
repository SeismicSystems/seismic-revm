
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract DERIVEAESKEY {
    function AESEncrypt(bytes32 aes_key, bytes32 nonce, bytes memory plaintext) public view returns (bytes memory) {
        // Address of the precompiled contract
        address AESEncryptAddr = address(0x67);

        // Concatenate secret key and public key
        bytes memory input = abi.encodePacked(aes_key, nonce, plaintext);

        // Call the precompiled contract
        (bool success, bytes memory output) = AESEncryptAddr.staticcall(input);

        // Ensure the call was successful
        require(success, "Precompile call failed");

        assembly {
            let len := mload(output)
            let data := add(output, 32)
            return(data, len)
        }
        
    }

    function testAESEncrypt() public view returns (bytes memory result) {
        bytes32 nonce = hex"11";
        bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        bytes memory plaintext = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
        result = AESEncrypt(aes_key, nonce, plaintext);
    }   
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testAESEncrypt() -> hex"1100000000000000d04ba6b89d92c660e4b8984b8072f6561d0fdd677f41a5f1ea516cefe163070e2a937a59b1dd4ff5708f6107ed101b493731c50c37f69bd516565d7972e85407885c73c8e459b7e59e3081f60551a35167e89ccd1df42836bb859540385897e9dcfc476b4349a9"

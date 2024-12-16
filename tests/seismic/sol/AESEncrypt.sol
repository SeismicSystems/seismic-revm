
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract DERIVEAESKEY {
    // TODO: fix out of gas error. its related to result being too long
    function AESEncrypt(bytes32 aes_key, bytes memory plaintext) public view returns (bytes memory result) {
        // Address of the precompiled contract
        address AESEncryptAddr = address(0x67);

        // TODO: nonce should come from the precompile, not be created here
        uint64 nonce = 17;

        // Concatenate secret key and public key
        bytes memory input = abi.encodePacked(aes_key, nonce, plaintext);

        // Call the precompiled contract
        (bool success, bytes memory output) = AESEncryptAddr.staticcall(input);

        // Ensure the call was successful
       
        require(success, "Precompile call failed");

        
        // Decode the result
        assembly {
            result := add(output, 32)
        }
        
    }

    function testAESEncrypt() public view returns (bytes memory result) {
        // require(false);
        bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        bytes memory plaintext = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
        result = AESEncrypt(aes_key, plaintext);
    }   
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testAESEncrypt()

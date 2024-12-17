
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract DERIVEAESKEY {
    // TODO: fix out of gas error. its related to result being too long
    function AESEncrypt(bytes32 aes_key, bytes memory plaintext) public view returns (bytes memory) {
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

        assembly {
            let len := mload(output)
            let data := add(output, 32)
            return(data, len)
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
// testAESEncrypt() -> hex"0000000000000011f577b2b34b7dbafad6647accfaa9194d7a39c839e618fdbe9fc304691385c6fdcb1a8bf1c84560871726c31334884d85b463b0d9930c50370b9cdcc492dfcfb232dd38f0b0beb1c75e6f5c07e3a9ad"


// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AESDECRYPT {
    function AESDecrypt(bytes32 aes_key, bytes32 nonce, bytes memory ciphertext) public view returns (bytes memory) {
        // Address of the precompiled contract
        address AESDecryptAddr = address(0x68);

        // Concatenate secret key and public key
        bytes memory input = abi.encodePacked(aes_key, nonce, ciphertext);

        // Call the precompiled contract
        (bool success, bytes memory output) = AESDecryptAddr.staticcall(input);

        // Ensure the call was successful
        require(success, "Precompile call failed");


        assembly {
            let len := mload(output)
            let data := add(output, 32)
            return(data, len)
        }
        
        
    }

    function testAESDecrypt() public view returns (bytes memory result) {
        bytes32 nonce = hex"11";
        bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        bytes memory ciphertext = hex"d149a5bc9894c168edb293478d7cf9460c1dce736a57b2e9f24974f7f97b1f2602bb527199f567cd48b7593fd52823611f19ed241fdeb3cd0e4e45616af04cdab1573e61c20f02e31eab10ab0f570f";
        result = AESDecrypt(aes_key, nonce, ciphertext);
    }   
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testAESDecrypt() -> hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f"

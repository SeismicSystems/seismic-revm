// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AESDECRYPT {
    function AESDecrypt(bytes32 aes_key, uint256 nonce, bytes memory ciphertext) public view returns (bytes memory) {
        // Address of the precompiled contract
        address AESDecryptAddr = address(0x68);

        // Concatenate secret key, nonce, and ciphertext
        bytes memory input = abi.encodePacked(aes_key, nonce, ciphertext);

        // Call the precompiled contract
        (bool success, bytes memory output) = AESDecryptAddr.staticcall(input);

        // Ensure the call was successful
        require(success, "Precompile call failed");

        // Copy the output into a new bytes array and return it
        bytes memory result = new bytes(output.length);
        assembly {
            let len := mload(output)
            let data := add(output, 32)

            // Copy the content of `output` into `result`
            for { let i := 0 } lt(i, len) { i := add(i, 32) } { mstore(add(result, add(32, i)), mload(add(data, i))) }

            // Set the length of the result
            mstore(result, len)
        }

        return result;
    }

    function testAESDecrypt() public view returns (bytes memory result) {
        uint256 nonce = 17;
        bytes32 aes_key = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        bytes memory ciphertext =
            hex"f577b2b34b7dbafad6647accfaa9194d7a39c839e618fdbe9fc304691385c6fdcb1a8bf1c84560871726c31334884d85b463b0d9930c50370b9cdcc492dfcfb232dd38f0b0beb1c75e6f5c07e3a9ad";
        result = AESDecrypt(aes_key, nonce, ciphertext);
    }
}
// ====
// EVMVersion: >=mercury
// ====
// ----
// testAESDecrypt() -> hex"08c379a000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000016507265636f6d70696c652063616c6c206661696c656400000000000000000000"

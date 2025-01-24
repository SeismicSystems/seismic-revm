// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SEISMICRNG {
    function seismicRng() public view returns (bytes memory) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(bytes1(0x00));

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        // Ensure the call was successful
        require(success, "RNG Precompile call failed");

        assembly {
            let len := mload(output)
            let data := add(output, 32)
            return(data, len)
        }
    }

    function seismicRngPers(bytes32 pers) public view returns (bytes memory) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(pers);

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        
        // Ensure the call was successful
        require(success, "RNG Precompile call failed");
        
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
// seismicRng() -> 0xf8ccad14448fe0e553d7be21e443ff6b32e7fff1f5962c5630d94f07f30af177
// seismicRng() -> 0x4769d00c3b88a5b0c12c11920fc71cb77a6750989d25dfb7f6c64ff93612e324
// seismicRngPers(bytes32): 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef ->  0xffc30e88edc4fb9bb3aefc69a4243e81eb8b158ca5686c6edaeb177f7d276585

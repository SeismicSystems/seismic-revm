// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SEISMICRNG {
    function seismicRng() public view returns (bytes memory) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(bytes1(0x00));

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(abi.encodePacked(uint32(32),input));
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
        (bool success, bytes memory output) = rngPrecompile.staticcall(abi.encodePacked(uint32(32),input));
        
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
// seismicRng() -> 0x13aaf6e5be2b9c57c15b83533b83f44a48c9b482c2a647946cfd58080388111f
// seismicRng() -> 0x13aaf6e5be2b9c57c15b83533b83f44a48c9b482c2a647946cfd58080388111f 
// seismicRngPers(bytes32): 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef -> 0xcde7fca4bbc86bcfb3acc70d4243e1aa67786379174b6056c8fc07f3572403da 

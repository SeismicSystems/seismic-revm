// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SEISMICRNG {
    function seismicRng() public view returns (bytes32 result) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(bytes1(0x00));

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        // Ensure the call was successful
        require(success, "RNG Precompile call failed");

        assembly {
            result := mload(add(output, 32))
        }
        
    }

    function seismicRngPers(bytes32 pers) public view returns (bytes32 result) {
        address rngPrecompile = address(0x64);

        bytes memory input = bytes.concat(pers);

        // Call the precompile
        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        
        // Ensure the call was successful
        require(success, "RNG Precompile call failed");

        assembly {
            result := mload(add(output, 32))
        }
        
    }

    function test_seismicRngPers(bytes32 pers) public view returns (bytes32 result) {
        result = seismicRngPers(pers);
    }

}
// TODO: figure out how to test seismicRngPers() better, remove test_seismicRngPers()
// ====
// EVMVersion: >=mercury
// ====
// ----
// seismicRng() -> 0x7a40ad457b8ccfc0f97d740a2fc28a7a0eb2ab0cb1934dedb53c0d8b2139e897
// seismicRng() -> 0x19cfb3ebd326b1a312640e740f7e2eb70b192daccfeaef1979bb012d0b1d6507
// test_seismicRngPers(bytes32): 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef ->  0x4ceef969af9666a582ddbf3a898a6b9e5a38947152b241defd9a852f4fc5697b

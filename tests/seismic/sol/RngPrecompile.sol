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

}
// ====
// EVMVersion: >=mercury
// ====
// ----
// seismicRNG() -> 0x891848b6044647ec2b698357ee73fc5cc83e907c9b6575b74f2cf75882ccb445
// seismicRngPers(0x11) -> 0x891848b6044647ec2b698357ee73fc5cc83e907c9b6575b74f2cf75882ccb445
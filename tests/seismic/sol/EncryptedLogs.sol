// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @title AESLogger
 * @dev This contract demonstrates how to:
 *      - Allow anyone to submit a plaintext message for encryption,
 *      - Generate a fresh random nonce for each submission,
 *      - Emit an event containing {nonce, ciphertext},
 *      - Provide a function (owner-only) to decrypt a given nonce & ciphertext on-chain.
 *
 * Precompile Addresses:
 *  - RNG         at 0x64
 *  - AESEncrypt  at 0x67
 *  - AESDecrypt  at 0x68
 */
contract AESLogger {
    // -----------------------------------------------------------------------------------------
    // Storage
    // -----------------------------------------------------------------------------------------
    address public owner;

    suint256 AES_KEY;

    event EncryptedMessage(
        uint96 indexed nonce,
        bytes ciphertext
    );

    // -----------------------------------------------------------------------------------------
    // Constructor & Modifiers
    // -----------------------------------------------------------------------------------------
    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "Only owner can call this function");
        _;
    }

    // -----------------------------------------------------------------------------------------
    // Owner-Only Functions
    // -----------------------------------------------------------------------------------------
    
    /**
     * @notice Set a new AES key (suint256). Only the owner can update the key.
     */
    function setAESKey(suint256 _aesKey) external onlyOwner {
        AES_KEY = _aesKey;
    }

    function setAESKeyinternal(suint256 _aesKey) internal onlyOwner returns (suint256) {
        return _aesKey;
    }
    
    /**
     * @notice Decrypt a given nonce & ciphertext on-chain. Only the owner can call this.
     * @param nonce       Random nonce that was used during encryption.
     * @param ciphertext  The ciphertext from the `EncryptedMessage` event.
     * @return plaintext  The decrypted result.
     */
    function decrypt(
        uint96 nonce,
        bytes calldata ciphertext
    ) external view onlyOwner returns (bytes memory plaintext) {
        require(ciphertext.length > 0, "Ciphertext cannot be empty");
        suint256 aesKey = AES_KEY;
        // 1) Calculate total length of the input to the precompile:
        //    32 bytes for key + 12 bytes for nonce + ciphertext.length
        uint256 totalLen = 44 + ciphertext.length;

        // 2) Allocate a new bytes array in Solidity for the raw call data
        bytes memory input = new bytes(totalLen);

        assembly {
            // 'input' points to a 32-byte header (array length), so skip that:
            //   ptr = start of the actual byte contents.
            let ptr := add(input, 32)

            // 3.1) Store the 32-byte key at [ptr .. ptr+32)
            mstore(ptr, aesKey)

            // 3.2) Store the 12-byte nonce right after the key. We want the nonce
            //      to occupy bytes [32..44). But mstore writes a full 32 bytes.
            //      So we do a left-shift by 160 bits so that only the lower 96 bits
            //      (12 bytes) land at [32..44].
            let nonceOffset := add(ptr, 32)
            mstore(nonceOffset, shl(160, nonce))
            // The top 20 bytes of that 32-word are zero, the bottom 12 are the nonce.

            // 3.3) Copy the raw ciphertext after offset 44
            let cipherOffset := add(ptr, 44)
            // calldata layout: 'ciphertext' has a 32-byte length slot at the front,
            // so skip that with 'add(ciphertext.offset, 0x20)'.
            calldatacopy(
                cipherOffset,
                add(ciphertext.offset, 0x20),
                ciphertext.length
            )
        }

        // 4) Now we have exactly the raw bytes we want:
        //    [0..32):  AES_KEY
        //    [32..44): nonce
        //    [44..):   ciphertext
        (bool success, bytes memory output) = address(0x68).staticcall(input);
        require(success, "AES decrypt precompile call failed");

        return output;
    }

    // -----------------------------------------------------------------------------------------
    // Public Function
    // -----------------------------------------------------------------------------------------

    /**
     * @notice Allows anyone to submit a plaintext message, which is encrypted under the stored AES_KEY.
     * @param plaintext The bytes to encrypt.
     */
    function submitMessage(bytes calldata plaintext) external {
        uint96 nonce = _generateRandomNonce();           
        bytes memory ciphertext = _encrypt(nonce, plaintext);
        emit EncryptedMessage(nonce, abi.encodePacked(ciphertext));             
    }

    // -----------------------------------------------------------------------------------------
    // Internal Helpers
    // -----------------------------------------------------------------------------------------

    /**
     * @dev Calls the RNG precompile to get a random nonce.
     */
    function _generateRandomNonce() internal view returns (uint96) {
        address rngPrecompile = address(0x64);
        bytes memory input = bytes.concat(bytes1(0x00)); // Adjust if the RNG precompile requires different input

        (bool success, bytes memory output) = rngPrecompile.staticcall(input);
        require(success, "RNG Precompile call failed");

        bytes32 randomBytes;
        assembly {
            randomBytes := mload(add(output, 32))
        }

        return uint96(uint256(randomBytes));
    }

    /**
     * @dev Encrypts the given plaintext with {AES_KEY, nonce} using the AES encryption precompile at 0x67.
     */
    function _encrypt(
        uint96 nonce,
        bytes memory plaintext
    ) internal view returns (bytes memory ciphertext) {
        address AESEncryptAddr = address(0x67);
        bytes memory input = abi.encodePacked(AES_KEY, nonce, plaintext);

        (bool success, bytes memory output) = AESEncryptAddr.staticcall(input);
        require(success, "AES encrypt precompile call failed");
        require(output.length > 0, "Encryption call returned no output");

        return output;
    }
}

contract RShiftSuint {
    function test() public {
        uint256 a = 10;  // 1010 in binary
        uint256 b = 12;  // 1100 in binary

        assert(suint256(b) >> 1 == 6);
        assert(b >> 1 == 6);
    }
}
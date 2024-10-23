contract LShiftSuint {
    function test() public pure {
        uint256 a = 10;  // 1010 in binary

        assert(suint256(a) << 1 == 20);
        assert(a << 1 == 20);
    }
}
contract MulSuint {
    function test() public pure {
        uint256 a = 7;
        uint256 b = 4;
        assert(suint256(a) * suint256(b) == 28);
        assert(suint256(a) * b == 28);
        assert(a * suint256(b) == 28);
        assert(a * b == 28);
    }
}
contract AddSuint {
    function test() public {
        uint256 a = 5;
        uint256 b = 6;
        assert(suint256(a) + suint256(b) == 11);
        assert(suint256(a) + b == 11);
        assert(a + suint256(b) == 11);
        assert(a + b == 11);
    }
}
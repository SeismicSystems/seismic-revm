contract DivSuint {
    function test() public {
        uint256 a = 24;
        uint256 b = 3;
        assert(suint256(a) / suint256(b) == 8);
        assert(suint256(a) / b == 8);
        assert(a / suint256(b) == 8);
        assert(a / b == 8);
    }
}
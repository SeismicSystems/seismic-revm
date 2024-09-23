contract CastToSuint {
    function test() public {
        uint256 x = 0x123456789;
        suint256 y = 0x123456789;
        suint256 z = x;

        assert(y == z);
    }
}